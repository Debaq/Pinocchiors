//! `Mutex` que no queda envenenado.
//!
//! Con `std::sync::Mutex`, un pánico con el candado tomado (un caso raro del
//! solver o de OpenCASCADE dentro de un comando) lo deja envenenado y todo
//! `.lock().unwrap()` posterior entra en pánico: la app queda inutilizable
//! hasta reiniciarla. Este tipo devuelve el dato igual; lo que pudo quedar a
//! medias lo limpia quien atrapa el pánico (ver `AppState::after_panic`).
//!
//! `lock()` devuelve `Result<_, Infallible>` para que el código existente
//! (`.lock().unwrap()`) siga igual y nunca falle.

use std::convert::Infallible;
use std::sync::MutexGuard;

#[derive(Default)]
pub struct Mutex<T: ?Sized>(std::sync::Mutex<T>);

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self(std::sync::Mutex::new(value))
    }

    pub fn into_inner(self) -> T {
        self.0.into_inner().unwrap_or_else(|e| e.into_inner())
    }
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> Result<MutexGuard<'_, T>, Infallible> {
        Ok(self.0.lock().unwrap_or_else(|e| {
            self.0.clear_poison();
            e.into_inner()
        }))
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut().unwrap_or_else(|e| e.into_inner())
    }
}

impl<T: ?Sized + std::fmt::Debug> std::fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::Mutex;

    #[test]
    fn a_panic_with_the_lock_taken_does_not_poison_it() {
        let m = std::sync::Arc::new(Mutex::new(1));
        let m2 = m.clone();
        let r = std::thread::spawn(move || {
            let mut g = m2.lock().unwrap();
            *g = 2;
            panic!("a propósito");
        })
        .join();
        assert!(r.is_err());
        assert_eq!(*m.lock().unwrap(), 2);
        *m.lock().unwrap() = 3;
        assert_eq!(*m.lock().unwrap(), 3);
    }
}
