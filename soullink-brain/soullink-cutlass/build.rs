fn main() {
    #[cfg(feature = "cutlass")]
    {
        println!("cargo:rerun-if-changed=kernels/");

        cc::Build::new()
            .cuda(true)
            .cudart("shared")
            .flag("-gencode=arch=compute_89,code=sm_89")
            .flag("-std=c++17")
            .flag("-O3")
            .flag("-Xcompiler=-fPIC")
            .include("/usr/local/cuda/include")
            .include("/opt/cutlass/include")
            .include("/opt/cutlass/tools/util/include")
            .file("kernels/cosine_sim.cu")
            .file("kernels/topk.cu")
            .file("kernels/hnn_step.cu")
            .compile("soullink_cutlass_kernels");

        println!("cargo:rustc-link-lib=dylib=cudart");
        println!("cargo:rustc-link-lib=dylib=cublas");
    }
}