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
    /// 普通参数 -> 从 JSON body 反序列化
    Json { key: String, ty: Type },
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

/// snake_case -> camelCase，复刻 `rename_all = "camelCase"`。
fn to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = false;
    for ch in s.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    // 原版只用到 `rename_all = "camelCase"` 这一种参数形态（19 处）。
    let rename_camel = attr.to_string().contains("camelCase");
    let func = parse_macro_input!(item as ItemFn);
    expand_command(func, rename_camel)
}

fn expand_command(func: ItemFn, rename_camel: bool) -> TokenStream {
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
                // 未加 rename_all 的命令直接用字面参数名（原版靠 camelCase 参数名 +
                // #[allow(non_snake_case)] 对齐前端），加了的按 camelCase 转换。
                let key = if rename_camel { to_camel(&raw) } else { raw };
                injected.push(Injected::Json { key, ty });
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
            Injected::Json { key, ty } => bindings.push(quote! {
                let #bind: #ty = ::tauri::rpc::take_arg(&__args, #key)?;
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
