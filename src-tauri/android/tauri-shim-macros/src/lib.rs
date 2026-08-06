//! 复刻 tauri 的 `#[command]` 与 `generate_handler!`，使 cc-switch 的 293 个
//! 命令在 Android sidecar 上**一行不改**即可编译。
//!
//! 与原版的差异：原版把命令挂到 IPC，这里把命令挂到 `POST /rpc/{name}`。
//! 参数来源从 IPC payload 换成 JSON body，其余语义（camelCase 参数名、
//! State 注入、AppHandle 注入、Result -> String 错误）保持一致。

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, punctuated::Punctuated, FnArg, GenericArgument, ItemFn, Pat, Path,
    PathArguments, ReturnType, Token, Type,
};

/// 参数注入类型。
enum Injected {
    /// `State<'_, T>` -> 从状态表取 `Arc<T>`
    State(Type),
    /// `AppHandle` / `AppHandle<R>`
    AppHandle,
    /// `Window`
    Window,
    /// 普通参数 -> 从 JSON body 反序列化。`key` 是 tauri 语义下的
    /// camelCase 键名，`raw` 是 Rust 字面参数名（兜底键）。
    Json { key: String, raw: String, ty: Type },
}

/// 取类型路径的最后一段标识符名。
fn last_segment_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(r) => last_segment_ident(&r.elem),
        _ => None,
    }
}

/// 从 `State<'_, T>` 中抽出 `T`。
fn state_inner_type(ty: &Type) -> Option<Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    })
}

/// snake_case -> lowerCamelCase，复刻 tauri 对命令参数名的默认改写。
///
/// tauri 2.x 无论是否写 `rename_all`，都用 heck 的 `to_lower_camel_case`
/// 处理命令参数名（`rename_all = "camelCase"` 只是把默认值写明）。所以这里
/// 必须对全部参数无条件转换，否则 `app_type` 这类参数在前端发来的
/// `appType` 里永远取不到值。
///
/// 按下划线切词、丢弃空词、首词全小写、其余词首字母大写。cc-switch 的 97 个
/// 唯一参数名里没有连续大写、下划线接大写、字母接数字的形态，故该简化实现与
/// heck 逐字节等价（已全量核对）。前导下划线会被丢弃，这正是
/// `delete_mcp_server_in_config` 的 `_app` 能对上前端 `app` 的原因。
fn to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split('_').filter(|w| !w.is_empty()) {
        let mut chars = word.chars();
        let Some(first) = chars.next() else { continue };
        if out.is_empty() {
            out.extend(first.to_lowercase());
        } else {
            out.extend(first.to_uppercase());
        }
        out.push_str(chars.as_str());
    }
    out
}

#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    // 原版只出现空属性和 `rename_all = "camelCase"` 两种形态，二者语义相同
    // （camelCase 就是 tauri 的默认值），故属性内容无需参与参数名推导。
    // 真出现 `rename_all = "snake_case"` 这类反向改写时直接编译报错，
    // 避免静默按 camelCase 展开、留下运行期取不到参数的暗坑。
    let attr = attr.to_string();
    if attr.contains("rename_all") && !attr.contains("camelCase") {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "tauri-shim 只支持 rename_all = \"camelCase\"（即 tauri 默认值）",
        )
        .to_compile_error()
        .into();
    }
    let func = parse_macro_input!(item as ItemFn);
    expand_command(func)
}

fn expand_command(func: ItemFn) -> TokenStream {
    let name = func.sig.ident.clone();
    let is_async = func.sig.asyncness.is_some();
    let vis = func.vis.clone();
    let rpc_name = format_ident!("__rpc_{}", name);

    // 泛型命令（`<R: tauri::Runtime>`，共 3 处）需要显式实例化为 Wry。
    let has_type_generic = func
        .sig
        .generics
        .params
        .iter()
        .any(|p| matches!(p, syn::GenericParam::Type(_)));
    let turbofish = if has_type_generic {
        quote!(::<::tauri::Wry>)
    } else {
        quote!()
    };

    // 分类每个参数。
    let mut injected = Vec::new();
    for arg in func.sig.inputs.iter() {
        let FnArg::Typed(pt) = arg else {
            return syn::Error::new_spanned(arg, "命令不支持 self 参数")
                .to_compile_error()
                .into();
        };
        let ty = (*pt.ty).clone();
        match last_segment_ident(&ty).as_deref() {
            Some("State") => match state_inner_type(&ty) {
                Some(inner) => injected.push(Injected::State(inner)),
                None => {
                    return syn::Error::new_spanned(&ty, "无法解析 State 的内部类型")
                        .to_compile_error()
                        .into()
                }
            },
            Some("AppHandle") => injected.push(Injected::AppHandle),
            Some("Window") => injected.push(Injected::Window),
            _ => {
                let Pat::Ident(pat_ident) = &*pt.pat else {
                    return syn::Error::new_spanned(&pt.pat, "命令参数必须是简单标识符")
                        .to_compile_error()
                        .into();
                };
                let raw = pat_ident.ident.to_string();
                // 主键按 tauri 语义转 camelCase；字面参数名作兜底，容纳前端个别
                // 直接写下划线的调用点。已是 camelCase 的参数名转换后不变，
                // 两个键会重合，take_arg 去重后只查一次。
                let key = to_camel(&raw);
                injected.push(Injected::Json { key, raw, ty });
            }
        }
    }

    // 生成绑定与调用实参。
    let mut bindings = Vec::new();
    let mut call_args = Vec::new();
    for (i, inj) in injected.iter().enumerate() {
        let bind = format_ident!("__arg{}", i);
        match inj {
            Injected::State(inner) => bindings.push(quote! {
                let #bind: ::tauri::State<'static, #inner> = __ctx.state::<#inner>()?;
            }),
            Injected::AppHandle => bindings.push(quote! {
                let #bind = __ctx.app_handle().clone();
            }),
            Injected::Window => bindings.push(quote! {
                let #bind = __ctx.window();
            }),
            Injected::Json { key, raw, ty } => bindings.push(quote! {
                let #bind: #ty = ::tauri::rpc::take_arg(&__args, &[#key, #raw])?;
            }),
        }
        call_args.push(quote!(#bind));
    }

    let invoke = if is_async {
        quote!(#name #turbofish (#(#call_args),*).await)
    } else {
        quote!(#name #turbofish (#(#call_args),*))
    };

    // 返回值：`Result<T, E>` 统一 map_err 成 String；非 Result 直接序列化。
    let returns_result = match &func.sig.output {
        ReturnType::Type(_, ty) => last_segment_ident(ty).as_deref() == Some("Result"),
        ReturnType::Default => false,
    };
    let finish = if returns_result {
        quote! {
            let __out = #invoke.map_err(|__e| ::tauri::rpc::stringify_err(__e))?;
        }
    } else {
        quote! { let __out = #invoke; }
    };

    let expanded = quote! {
        #func

        #[doc(hidden)]
        #[allow(non_snake_case)]
        #vis fn #rpc_name(
            __ctx: ::tauri::rpc::RpcContext,
            __args: ::serde_json::Value,
        ) -> ::std::pin::Pin<::std::boxed::Box<
            dyn ::std::future::Future<
                Output = ::std::result::Result<::serde_json::Value, ::std::string::String>,
            > + Send + 'static,
        >> {
            ::std::boxed::Box::pin(async move {
                #(#bindings)*
                #finish
                ::serde_json::to_value(__out).map_err(|__e| __e.to_string())
            })
        }
    };
    expanded.into()
}

/// 复刻 `generate_handler![commands::a, commands::b, ...]`。
///
/// 把每个路径的最后一段 `foo` 重写为 `__rpc_foo`，并以命令名为键装进 `Handlers`。
#[proc_macro]
pub fn generate_handler(input: TokenStream) -> TokenStream {
    let paths =
        parse_macro_input!(input with Punctuated::<Path, Token![,]>::parse_terminated);

    let mut names = Vec::new();
    let mut rpc_paths = Vec::new();
    for path in paths.iter() {
        let Some(last) = path.segments.last() else {
            continue;
        };
        let cmd_name = last.ident.to_string();
        let mut rpc_path = path.clone();
        let n = rpc_path.segments.len();
        rpc_path.segments[n - 1].ident = format_ident!("__rpc_{}", last.ident);
        names.push(cmd_name);
        rpc_paths.push(rpc_path);
    }

    quote! {
        ::tauri::rpc::Handlers::from_pairs(::std::vec![
            #( (#names, #rpc_paths as ::tauri::rpc::HandlerFn) ),*
        ])
    }
    .into()
}

/// `to_camel` 是整个参数取值链路的唯一键名来源，改错会让 46 个命令静默取不到
/// 参数，所以在这里把与 heck `to_lower_camel_case` 的等价性钉死。
/// 用例全部取自 cc-switch 真实参数名。
#[cfg(test)]
mod tests {
    use super::to_camel;

    #[test]
    fn snake_case_becomes_lower_camel() {
        assert_eq!(to_camel("app_type"), "appType");
        assert_eq!(to_camel("provider_id"), "providerId");
        assert_eq!(to_camel("access_key_id"), "accessKeyId");
        assert_eq!(to_camel("wsl_shell_by_tool"), "wslShellByTool");
        assert_eq!(to_camel("team_organization_id"), "teamOrganizationId");
        assert_eq!(to_camel("cache_creation_cost"), "cacheCreationCost");
    }

    #[test]
    fn single_word_stays_lowercase() {
        assert_eq!(to_camel("enabled"), "enabled");
        assert_eq!(to_camel("id"), "id");
    }

    #[test]
    fn already_camel_is_idempotent() {
        // 前端本就发 camelCase；若某个命令的 Rust 参数名已是 camelCase，
        // 转换必须是恒等映射，否则主键会跑偏。
        for s in ["providerId", "appType", "modelId"] {
            assert_eq!(to_camel(s), s);
            assert_eq!(to_camel(&to_camel(s)), s);
        }
    }

    #[test]
    fn leading_underscore_is_dropped() {
        // `delete_mcp_server_in_config(_app: String)` 对应前端 mcp.ts 发的 `app`。
        assert_eq!(to_camel("_app"), "app");
        assert_eq!(to_camel("_state"), "state");
    }

    #[test]
    fn empty_words_are_skipped() {
        assert_eq!(to_camel("a__b"), "aB");
        assert_eq!(to_camel("__"), "");
        assert_eq!(to_camel(""), "");
    }
}
