//! Kernel template definitions and source code generation.
//!
//! Each kernel type has a template that generates optimized Rust source code.

use scirust_inference::layers::ActivationType;

/// Types of kernels that can be JIT-compiled.
#[derive(Debug, Clone, PartialEq)]
pub enum KernelType {
    /// GEMM: C = A @ B with fixed tile size
    Gemm { m: usize, n: usize, k: usize, tile: usize },
    /// Element-wise ReLU
    Relu,
    /// Element-wise Sigmoid
    Sigmoid,
    /// Softmax over last axis
    Softmax { batch: usize, features: usize },
    /// Fused linear + activation
    FusedLinearAct { m: usize, k: usize, n: usize, activation: String },
}

impl std::fmt::Display for KernelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KernelType::Gemm { m, n, k, tile } => write!(f, "gemm_{}x{}x{}_t{}", m, n, k, tile),
            KernelType::Relu => write!(f, "relu"),
            KernelType::Sigmoid => write!(f, "sigmoid"),
            KernelType::Softmax { batch, features } => write!(f, "softmax_{}x{}", batch, features),
            KernelType::FusedLinearAct { m, k, n, .. } => write!(f, "fused_linear_{}x{}x{}", m, k, n),
        }
    }
}

/// A kernel template with its source code and entry point.
#[derive(Debug, Clone)]
pub struct KernelTemplate {
    pub kernel_type: KernelType,
    pub source: String,
    pub function_name: String,
}

impl KernelTemplate {
    /// Create a new kernel template.
    pub fn new(kernel_type: KernelType, source: String, function_name: &str) -> Self {
        Self { kernel_type, source, function_name: function_name.to_string() }
    }
}

/// Generate an optimized GEMM kernel source with tiling.
pub fn generate_gemm_kernel(m: usize, n: usize, k: usize, tile: usize) -> KernelTemplate {
    let fn_name = format!("gemm_{}x{}x{}_t{}", m, n, k, tile);
    // Use the effective tile size (min of tile and actual dimension)
    let tm = tile.min(m);
    let tn = tile.min(n);
    let tk = tile.min(k);

    let source = format!(r#"
#![no_std]
extern "C" fn {fn_name}(a: *const f32, b: *const f32, c: *mut f32) {{
    let m: usize = {m};
    let n: usize = {n};
    let k: usize = {k};
    let tm: usize = {tm};
    let tn: usize = {tn};
    let tk: usize = {tk};

    unsafe {{
        for i0 in (0..m).step_by(tm) {{
            let imax = (i0 + tm).min(m);
            for j0 in (0..n).step_by(tn) {{
                let jmax = (j0 + tn).min(n);
                for k0 in (0..k).step_by(tk) {{
                    let kmax = (k0 + tk).min(k);
                    for i in i0..imax {{
                        for kk in k0..kmax {{
                            let aik = *a.add(i * k + kk);
                            for j in j0..jmax {{
                                *c.add(i * n + j) += aik * *b.add(kk * n + j);
                            }}
                        }}
                    }}
                }}
            }}
        }}
    }}
}}
"#);

    KernelTemplate::new(
        KernelType::Gemm { m, n, k, tile },
        source,
        &fn_name,
    )
}

/// Generate a ReLU kernel source.
pub fn generate_relu_kernel() -> KernelTemplate {
    let source = r#"
#![no_std]
extern "C" fn relu_kernel(data: *mut f32, len: usize) {
    unsafe {
        for i in 0..len {
            let x = *data.add(i);
            if x < 0.0 { *data.add(i) = 0.0; }
        }
    }
}
"#.to_string();
    KernelTemplate::new(KernelType::Relu, source, "relu_kernel")
}

/// Generate a Sigmoid kernel source.
pub fn generate_sigmoid_kernel() -> KernelTemplate {
    let source = r#"
#![no_std]
extern "C" fn sigmoid_kernel(data: *mut f32, len: usize) {
    unsafe {
        for i in 0..len {
            let x = *data.add(i);
            *data.add(i) = 1.0 / (1.0 + (-x).exp());
        }
    }
}
"#.to_string();
    KernelTemplate::new(KernelType::Sigmoid, source, "sigmoid_kernel")
}

/// Generate a Softmax kernel source.
pub fn generate_softmax_kernel(batch: usize, features: usize) -> KernelTemplate {
    let fn_name = format!("softmax_{}x{}", batch, features);
    let source = format!(r#"
#![no_std]
extern "C" fn {fn_name}(data: *mut f32) {{
    let batch: usize = {batch};
    let features: usize = {features};
    unsafe {{
        for b in 0..batch {{
            let start = b * features;
            let mut max_val = *data.add(start);
            for i in 1..features {{
                let v = *data.add(start + i);
                if v > max_val {{ max_val = v; }}
            }}
            let mut sum = 0.0f32;
            for i in 0..features {{
                let e = (*data.add(start + i) - max_val).exp();
                *data.add(start + i) = e;
                sum += e;
            }}
            let inv_sum = 1.0 / sum;
            for i in 0..features {{
                *data.add(start + i) *= inv_sum;
            }}
        }}
    }}
}}
"#);
    KernelTemplate::new(KernelType::Softmax { batch, features }, source, &fn_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemm_template_creation() {
        let tpl = generate_gemm_kernel(64, 64, 64, 16);
        assert!(tpl.source.contains("gemm_64x64x64"));
        assert!(tpl.source.contains("extern \"C\""));
        assert!(matches!(tpl.kernel_type, KernelType::Gemm { .. }));
    }

    #[test]
    fn test_relu_template() {
        let tpl = generate_relu_kernel();
        assert!(tpl.source.contains("relu_kernel"));
        assert!(tpl.source.contains("if x < 0.0"));
    }

    #[test]
    fn test_sigmoid_template() {
        let tpl = generate_sigmoid_kernel();
        assert!(tpl.source.contains("sigmoid_kernel"));
        assert!(tpl.source.contains("1.0 / (1.0 + (-x).exp())"));
    }

    #[test]
    fn test_softmax_template() {
        let tpl = generate_softmax_kernel(8, 16);
        assert!(tpl.source.contains("softmax_8x16"));
    }

    #[test]
    fn test_kernel_type_display() {
        let gemm = KernelType::Gemm { m: 128, n: 64, k: 32, tile: 16 };
        assert_eq!(format!("{}", gemm), "gemm_128x64x32_t16");

        let relu = KernelType::Relu;
        assert_eq!(format!("{}", relu), "relu");
    }

    #[test]
    fn test_tile_size_adaptive() {
        // When m < tile, the template should adapt
        let tpl = generate_gemm_kernel(8, 256, 64, 64);
        assert!(tpl.source.contains("let tm: usize = 8")); // tm = min(64, 8) = 8
    }
}
