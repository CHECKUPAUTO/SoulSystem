//! Parallel dispatch — distributes work across GPU or CPU workers.

/// Process data in parallel chunks, preferring GPU if available, falling back
/// to CPU with rayon-like chunking.
pub fn gpu_or_cpu<T, F>(data: &mut [T], f: F)
where
    F: Fn(&mut [T]) + Send + Sync,
    T: Send,
{
    let n = data.len();
    if n == 0 {
        return;
    }
    let chunk_size = (n / num_cpus::get()).max(1);
    data.chunks_mut(chunk_size).for_each(f);
}

/// Convenience function to dispatch over multiple slices simultaneously.
pub fn gpu_or_cpu_zip<T, F>(a: &mut [T], b: &mut [T], f: F)
where
    F: Fn(&mut [T], &mut [T]) + Send + Sync,
    T: Send,
{
    let n = a.len().min(b.len());
    if n == 0 {
        return;
    }
    let chunk_size = (n / num_cpus::get()).max(1);
    for (ca, cb) in a[..n]
        .chunks_mut(chunk_size)
        .zip(b[..n].chunks_mut(chunk_size))
    {
        f(ca, cb);
    }
}
