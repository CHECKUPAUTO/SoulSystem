//! GPU integration test — validates CUTLASS kernels on RTX 4060 (sm_89)
//! Run: cargo test -p soullink-cutlass --features cutlass --test gpu_integration -- --nocapture

#[cfg(feature = "cutlass")]
mod gpu_tests {
    use soullink_cutlass::{GpuTensor, CudaStream, cosine_sim_batch, topk};
    use std::time::Instant;

    #[test]
    fn bench_rag_similarity_storm() {
        let _ctx = cust::quick_init().expect("CUDA context");

        let dim = 768;
        let num_docs = 1_000;

        let query_data = vec![0.5f32; dim];
        let db_data = vec![0.2f32; dim * num_docs];

        let q_gpu = GpuTensor::from_slice_f32(&query_data, &[1, dim]).expect("GPU alloc query");
        let db_gpu = GpuTensor::from_slice_f32(&db_data, &[num_docs, dim]).expect("GPU alloc db");
        let stream = CudaStream::new().expect("CUDA stream");

        // Warm-up: 10 iterations to heat cuBLAS and GPU caches
        for _ in 0..10 {
            let sim = cosine_sim_batch(&q_gpu, &db_gpu, &stream).expect("warmup sim");
            let _ = topk(&sim, 10, &stream).expect("warmup topk");
        }
        stream.synchronize().expect("warmup sync");

        println!("🚀 Bench RAG: 1 query × {} docs (dim={})", num_docs, dim);
        let start = Instant::now();

        let similarities = cosine_sim_batch(&q_gpu, &db_gpu, &stream).expect("cosine_sim");
        let (values, _indices) = topk(&similarities, 10, &stream).expect("topk");
        stream.synchronize().expect("sync");
        let duration = start.elapsed();

        let top_vals = values.to_vec_f32().unwrap();
        println!("✅ Top-1 Score: {:.4}", top_vals[0]);
        println!("⏱️  Warm Sim + TopK: {:?}", duration);
        println!("📊 Throughput: {:.0} queries/sec", 1.0 / duration.as_secs_f64());
        assert!(top_vals[0] > 0.0);
    }

    #[test]
    fn bench_hnn_verlet_step() {
        let _ctx = cust::quick_init().expect("CUDA context");

        let num_traj = 10_000;
        let dim = 3;

        let q_data = vec![1.0f32; num_traj * dim];
        let p_data = vec![0.1f32; num_traj * dim];
        let masses = vec![1.0f32; num_traj];

        let mut q = GpuTensor::from_slice_f32(&q_data, &[num_traj, dim]).expect("q alloc");
        let mut p = GpuTensor::from_slice_f32(&p_data, &[num_traj, dim]).expect("p alloc");
        let m = GpuTensor::from_slice_f32(&masses, &[num_traj]).expect("masses alloc");
        let stream = CudaStream::new().expect("stream");

        // Warm-up
        for _ in 0..10 {
            soullink_cutlass::hnn_verlet_step(&mut q, &mut p, &m, 0.1, 0.01, 0.01, &stream).expect("warmup");
        }
        stream.synchronize().expect("warmup sync");

        println!("🧠 HNN Verlet: {} trajectories × {} dim", num_traj, dim);
        let start = Instant::now();

        for _ in 0..1000 {
            soullink_cutlass::hnn_verlet_step(&mut q, &mut p, &m, 0.1, 0.01, 0.01, &stream).expect("verlet");
        }
        stream.synchronize().expect("sync");
        let duration = start.elapsed();

        println!("⏱️  1000 warm steps: {:?}", duration);
        println!("📊 {} steps/sec", 1000.0 / duration.as_secs_f64());
    }

    #[test]
    fn bench_rag_similarity_large() {
        let _ctx = cust::quick_init().expect("CUDA context");

        // Larger scale: 100 queries × 10K docs
        let dim = 768;
        let num_queries = 100;
        let num_docs = 10_000;

        let query_data = vec![0.5f32; num_queries * dim];
        let db_data = vec![0.2f32; num_docs * dim];

        let q_gpu = GpuTensor::from_slice_f32(&query_data, &[num_queries, dim]).expect("query alloc");
        let db_gpu = GpuTensor::from_slice_f32(&db_data, &[num_docs, dim]).expect("db alloc");
        let stream = CudaStream::new().expect("stream");

        // Warm-up
        for _ in 0..5 {
            let _ = cosine_sim_batch(&q_gpu, &db_gpu, &stream).expect("warmup");
        }
        stream.synchronize().expect("warmup sync");

        println!("🔥 Bench RAG Large: {} queries × {} docs (dim={})", num_queries, num_docs, dim);
        let start = Instant::now();

        let similarities = cosine_sim_batch(&q_gpu, &db_gpu, &stream).expect("sim");
        let (values, _) = topk(&similarities, 10, &stream).expect("topk");
        stream.synchronize().expect("sync");
        let duration = start.elapsed();

        let top_vals = values.to_vec_f32().unwrap();
        println!("✅ Top-1 Score: {:.4}", top_vals[0]);
        println!("⏱️  {} queries × {} docs: {:?}", num_queries, num_docs, duration);
        println!("📊 Throughput: {:.0} queries/sec", num_queries as f64 / duration.as_secs_f64());
        assert!(top_vals[0] > 0.0);
    }
}