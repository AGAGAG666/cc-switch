//! `State<'_, T>` 与类型化状态表。
//!
//! 原版 State 由 Tauri 的 `Manager::manage` 注册、按 TypeId 取出。
//! 这里用同样的 TypeId 映射，语义一致：注册过就能取到，没注册就是调用方 bug。

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, RwLock};

/// 与 `tauri::State` 等价的包装。
///
/// 原版是 `&T` 的零成本包装；这里持 `Arc<T>` 以便脱离请求生命周期，
/// 但对外仍表现为 `Deref<Target = T>`，所以 `state.inner()`、`&*state`、
/// 以及 `state.field` 这类原版写法全部照旧可用。
pub struct State<'a, T: Send + Sync + 'static> {
    inner: Arc<T>,
    _marker: std::marker::PhantomData<&'a T>,
}

impl<T: Send + Sync + 'static> State<'_, T> {
    pub fn new(inner: Arc<T>) -> Self {
        Self {
            inner,
            _marker: std::marker::PhantomData,
        }
    }

    /// 复刻 `State::inner()`：拿到底层 `&T`（原版 37 处调用）。
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// 便于把状态所有权带进 spawn 的闭包。
    pub fn to_arc(&self) -> Arc<T> {
        Arc::clone(&self.inner)
    }
}

impl<T: Send + Sync + 'static> Deref for State<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: Send + Sync + 'static> Clone for State<'_, T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _marker: std::marker::PhantomData,
        }
    }
}

/// 进程级状态表，对应原版 `app.manage(...)`。
#[derive(Default)]
pub struct StateMap {
    map: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl StateMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn manage<T: Send + Sync + 'static>(&self, value: T) {
        self.map
            .write()
            .expect("状态表读写锁被污染")
            .insert(TypeId::of::<T>(), Arc::new(value));
    }

    pub fn try_get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        let guard = self.map.read().expect("状态表读写锁被污染");
        guard
            .get(&TypeId::of::<T>())
            .and_then(|v| Arc::clone(v).downcast::<T>().ok())
    }
}
