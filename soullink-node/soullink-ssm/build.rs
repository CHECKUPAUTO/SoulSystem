/// Build script for Soullink-SSM.
///
/// When the `cuda` feature is enabled, compiles CUDA kernels
/// using nvcc and links them as a static library.
///
/// Usage:
///   cargo build --features cuda          # build with CUDA support
///   cargo build                          # build without CUDA (Rust fallback)
///
/// Requirements for CUDA:
///   - CUDA Toolkit (nvcc) installed and on PATH
///   - GPU with compute capability 6.0+ (Pascal or newer)
fn main() {
    // Only compile CUDA kernels when the `cuda` feature is enabled
    #[cfg(feature = "cuda")]
    {
        use std::path::Path;

        let cuda_toolkit = match std::env::var("CUDA_HOME").or_else(|_| std::env::var("CUDA_PATH"))
        {
            Ok(path) => path,
            Err(_) => {
                // Try to find nvcc on PATH
                let has_nvcc = std::process::Command::new("nvcc")
                    .arg("--version")
                    .output()
                    .is_ok();
                if !has_nvcc {
                    panic!(
                        "CUDA feature enabled but nvcc not found. \
                         Install CUDA Toolkit or build without --features cuda"
                    );
                }
                String::new()
            }
        };

        let kernels_dir = Path::new("kernels");
        let kernels_dir = if kernels_dir.exists() {
            kernels_dir.to_path_buf()
        } else {
            Path::new("/root/soullink-ssm/kernels").to_path_buf()
        };

        // Verify kernels directory exists
        if !kernels_dir.exists() {
            panic!(
                "kernels/ directory not found at {:?}",
                kernels_dir.canonicalize()
            );
        }

        // Compile each .cu file
        let mut cc_build = cc::Build::new();
        cc_build
            .cuda(true)
            .flag("-arch=sm_60") // Pascal minimum
            .flag("-O3")
            .flag("-use_fast_math")
            .flag("-Wno-deprecated-gpu-targets");

        // Add CUDA include path if CUDA_HOME is set
        if !cuda_toolkit.is_empty() {
            let include_path = format!("{}/include", cuda_toolkit);
            if Path::new(&include_path).exists() {
                cc_build.include(&include_path);
            }
        }

        // Add all .cu files from kernels/
        for entry in std::fs::read_dir(kernels_dir).expect("Failed to read kernels/ directory") {
            let entry = entry.expect("Failed to read kernel file entry");
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "cu") {
                println!("cargo:rerun-if-changed={}", path.display());
                cc_build.file(&path);
                println!("cargo:warning=Compiling CUDA kernel: {}", path.display());
            }
        }

        cc_build.compile("soullink_ssm_cuda");

        println!("cargo:warning=CUDA kernels compiled successfully");
    }

    #[cfg(not(feature = "cuda"))]
    {
        println!("cargo:warning=CUDA feature disabled — using Rust fallback");
        println!("cargo:warning=Enable with: cargo build --features cuda");
    }
}
