//! Buffer pooling for reduced allocations.
//!
//! Provides reusable buffer pools for common operations to avoid
//! repeated allocations in hot paths.

use std::cell::RefCell;
use std::collections::VecDeque;

/// A pool of reusable Vec buffers.
///
/// Buffers are returned to the pool when dropped, allowing reuse
/// without new allocations.
pub struct VecPool<T> {
    buffers: RefCell<VecDeque<Vec<T>>>,
    max_buffers: usize,
    initial_capacity: usize,
}

impl<T> VecPool<T> {
    /// Create a new buffer pool.
    ///
    /// # Arguments
    /// * `max_buffers` - Maximum number of buffers to keep in pool
    /// * `initial_capacity` - Initial capacity for new buffers
    pub fn new(max_buffers: usize, initial_capacity: usize) -> Self {
        Self {
            buffers: RefCell::new(VecDeque::with_capacity(max_buffers)),
            max_buffers,
            initial_capacity,
        }
    }

    /// Get a buffer from the pool, or create a new one.
    pub fn get(&self) -> PooledVec<'_, T> {
        let buffer = self.buffers.borrow_mut().pop_front().unwrap_or_else(|| {
            Vec::with_capacity(self.initial_capacity)
        });
        PooledVec {
            buffer: Some(buffer),
            pool: self,
        }
    }

    /// Return a buffer to the pool.
    fn return_buffer(&self, mut buffer: Vec<T>) {
        buffer.clear();
        let mut buffers = self.buffers.borrow_mut();
        if buffers.len() < self.max_buffers {
            buffers.push_back(buffer);
        }
        // Otherwise, just drop the buffer
    }

    /// Number of buffers currently in pool.
    pub fn available(&self) -> usize {
        self.buffers.borrow().len()
    }
}

impl<T> Default for VecPool<T> {
    fn default() -> Self {
        Self::new(16, 64)
    }
}

/// A Vec borrowed from a pool.
///
/// When dropped, the buffer is returned to the pool for reuse.
pub struct PooledVec<'a, T> {
    buffer: Option<Vec<T>>,
    pool: &'a VecPool<T>,
}

impl<'a, T> PooledVec<'a, T> {
    /// Get a reference to the underlying Vec.
    pub fn as_vec(&self) -> &Vec<T> {
        self.buffer.as_ref().unwrap()
    }

    /// Get a mutable reference to the underlying Vec.
    pub fn as_vec_mut(&mut self) -> &mut Vec<T> {
        self.buffer.as_mut().unwrap()
    }

    /// Take ownership of the buffer (it won't return to pool).
    pub fn take(mut self) -> Vec<T> {
        self.buffer.take().unwrap()
    }
}

impl<'a, T> std::ops::Deref for PooledVec<'a, T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        self.buffer.as_ref().unwrap()
    }
}

impl<'a, T> std::ops::DerefMut for PooledVec<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer.as_mut().unwrap()
    }
}

impl<'a, T> Drop for PooledVec<'a, T> {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            self.pool.return_buffer(buffer);
        }
    }
}

// Thread-local pool for common buffer types.
thread_local! {
    /// Pool for f64 vectors (used in smoothing, position computation)
    pub static F64_POOL: VecPool<f64> = VecPool::new(8, 256);

    /// Pool for usize vectors (used in adjacency lists, indices)
    pub static USIZE_POOL: VecPool<usize> = VecPool::new(8, 64);
}

/// Get a pooled f64 buffer.
pub fn get_f64_buffer() -> PooledVec<'static, f64> {
    F64_POOL.with(|pool| {
        // Safety: we're returning a reference to thread-local storage
        // which lives for 'static within this thread
        unsafe { std::mem::transmute(pool.get()) }
    })
}

/// Get a pooled usize buffer.
pub fn get_usize_buffer() -> PooledVec<'static, usize> {
    USIZE_POOL.with(|pool| {
        unsafe { std::mem::transmute(pool.get()) }
    })
}

/// Scratch space for algorithms that need temporary storage.
#[derive(Default)]
pub struct ScratchSpace {
    /// Temporary f64 storage
    pub f64_scratch: Vec<f64>,
    /// Temporary usize storage
    pub usize_scratch: Vec<usize>,
    /// Temporary bool storage
    pub bool_scratch: Vec<bool>,
}

impl ScratchSpace {
    /// Create scratch space with given capacities.
    pub fn with_capacity(f64_cap: usize, usize_cap: usize, bool_cap: usize) -> Self {
        Self {
            f64_scratch: Vec::with_capacity(f64_cap),
            usize_scratch: Vec::with_capacity(usize_cap),
            bool_scratch: Vec::with_capacity(bool_cap),
        }
    }

    /// Clear all scratch buffers.
    pub fn clear(&mut self) {
        self.f64_scratch.clear();
        self.usize_scratch.clear();
        self.bool_scratch.clear();
    }

    /// Resize f64 scratch to given size, filling with value.
    pub fn resize_f64(&mut self, size: usize, value: f64) {
        self.f64_scratch.clear();
        self.f64_scratch.resize(size, value);
    }

    /// Resize usize scratch to given size, filling with value.
    pub fn resize_usize(&mut self, size: usize, value: usize) {
        self.usize_scratch.clear();
        self.usize_scratch.resize(size, value);
    }

    /// Resize bool scratch to given size, filling with value.
    pub fn resize_bool(&mut self, size: usize, value: bool) {
        self.bool_scratch.clear();
        self.bool_scratch.resize(size, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec_pool_basic() {
        let pool: VecPool<i32> = VecPool::new(4, 16);

        assert_eq!(pool.available(), 0);

        {
            let mut buf = pool.get();
            buf.push(1);
            buf.push(2);
            buf.push(3);
            assert_eq!(buf.len(), 3);
        }

        // Buffer should be returned to pool
        assert_eq!(pool.available(), 1);

        // Getting again should reuse
        let buf = pool.get();
        assert!(buf.is_empty()); // Was cleared
        assert!(buf.capacity() >= 3); // But capacity preserved
    }

    #[test]
    fn test_pool_max_buffers() {
        let pool: VecPool<i32> = VecPool::new(2, 8);

        // Create 3 buffers
        let b1 = pool.get();
        let b2 = pool.get();
        let b3 = pool.get();

        drop(b1);
        drop(b2);
        drop(b3);

        // Only 2 should be in pool (max_buffers = 2)
        assert_eq!(pool.available(), 2);
    }

    #[test]
    fn test_pooled_vec_take() {
        let pool: VecPool<i32> = VecPool::new(4, 16);

        let mut buf = pool.get();
        buf.push(42);
        let vec = buf.take();

        assert_eq!(vec, vec![42]);
        // Buffer was taken, not returned
        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn test_scratch_space() {
        let mut scratch = ScratchSpace::with_capacity(100, 50, 25);

        scratch.resize_f64(10, 0.0);
        assert_eq!(scratch.f64_scratch.len(), 10);

        scratch.resize_bool(5, true);
        assert_eq!(scratch.bool_scratch.len(), 5);
        assert!(scratch.bool_scratch.iter().all(|&b| b));

        scratch.clear();
        assert!(scratch.f64_scratch.is_empty());
        assert!(scratch.bool_scratch.is_empty());
    }

    #[test]
    fn test_thread_local_pools() {
        let mut buf = get_f64_buffer();
        buf.push(1.0);
        buf.push(2.0);
        assert_eq!(buf.len(), 2);
        drop(buf);

        let buf = get_f64_buffer();
        assert!(buf.is_empty());
    }
}
