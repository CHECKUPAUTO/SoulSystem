fn main() {
    #[cfg(feature = "dali")]
    {
        let dali_inc = "/usr/local/lib/python3.13/dist-packages/nvidia/dali/include";
        let dali_lib = "/usr/local/lib/python3.13/dist-packages/nvidia/dali";

        cxx_build::bridge("src/pipeline.rs")
            .cpp(true)
            .std("c++17")
            .flag("-fPIC")
            .flag("-O2")
            .include(dali_inc)
            .include("cxx-bridge")
            .file("cxx-bridge/dali_wrapper.cpp")
            .compile("soullink_dali");

        println!("cargo:rustc-link-search=native={}", dali_lib);
        println!("cargo:rustc-link-lib=dylib=dali");
        println!("cargo:rustc-link-lib=dylib=dali_core");
        println!("cargo:rustc-link-lib=dylib=dl");

        println!("cargo:rerun-if-changed=cxx-bridge/");
        println!("cargo:rerun-if-changed=src/pipeline.rs");
    }
}