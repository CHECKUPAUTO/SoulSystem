# JIT Agentic Engine 🚀

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org)

JIT Agentic Engine is a high-performance framework designed to empower AI agents with the ability to generate, compile, and execute Rust "skills" on the fly. By leveraging Just-In-Time (JIT) compilation, it bridges the gap between the flexibility of interpreted logic and the raw performance of native code.

## 🌟 Why JIT for Agents?

AI agents often need to perform specialized computations (e.g., data processing, cryptographic operations, custom kernels) that are inefficient in interpreted languages like Python or through high-level LLM reasoning.

JIT Agentic Engine allows:
- **On-the-fly Optimization**: Agents generate optimized Rust kernels tailored to the exact task at hand.
- **Native Performance**: Compiled skills run at native speed with full CPU optimizations.
- **Zero-Copy Interop**: Efficient memory sharing between the host engine and dynamic skills.
- **Safe Dynamic Loading**: Hot-swap skills without restarting the main application.

## 📋 Table of Contents
- [Features](#features)
- [Project Structure](#project-structure)
- [Prerequisites](#prerequisites)
- [Installation](#installation)
- [Usage Example](#usage-example)
- [Architecture](#architecture)
- [Contributing](#contributing)
- [License](#license)

## ✨ Features

- **Automated Compilation**: Seamless integration with `cargo` to compile Rust source strings into shared libraries.
- **Dynamic Skill Loading**: Load `.so`/`.dll` files at runtime using `libloading`.
- **Performance Focused**: Built-in support for Thin LTO and `target-cpu=native`.
- **Robust ABI**: Standardized FFI bridge for stable host-guest communication.
- **Zero-Copy Buffer Sharing**: Execute skills directly on host-provided memory.

## 📂 Project Structure

```text
.
├── crates/
│   ├── ffi_bridge/       # Shared ABI and types
│   ├── jit_compiler/     # Forge for compiling Rust source code
│   ├── dynamic_loader/   # Runtime for loading and executing skills
│   └── jit_demo/         # End-to-end integration demo
├── templates/            # Guidance for skill generation
├── docs/                 # Detailed documentation
├── Cargo.toml            # Workspace configuration
└── README.md             # This file
```

## ⚙️ Prerequisites

- **Rust**: Version 1.70 or higher.
- **Cargo**: Required for building the engine and JIT compilation of skills.
- **Build Tools**: Standard C/C++ build tools (gcc/clang/msvc) for your platform.

## 🚀 Installation

Clone the repository and build the workspace:

```bash
git clone https://github.com/CHECKUPAUTO/jit-agentic-engine.git
cd jit-agentic-engine
cargo build --release
```

## 💡 Usage Example

You can run the built-in demo to see the engine in action:

```bash
cargo run -p jit_demo
```

### Programmatic Usage

```rust
use jit_compiler::{Forge, JitConfig};
use dynamic_loader::SkillManager;

// 1. Your generated Rust code
let source = r#"
    #[no_mangle]
    pub extern "C" fn skill_execute(input_ptr: *const u8, output_ptr: *mut u8, len: usize) {
        // High performance logic here...
    }
"#;

// 2. Compile
let config = JitConfig::default();
let lib_path = Forge::compile_skill(source, &config)?;

// 3. Load and Run
let mut manager = SkillManager::new();
manager.swap_skill(&lib_path)?;
manager.execute(&input, &mut output)?;
```

## 🏗️ Architecture

The engine is split into three main layers:
1. **The Contract (`ffi_bridge`)**: Defines how data is passed.
2. **The Forge (`jit_compiler`)**: Handles the compilation pipeline.
3. **The Runtime (`dynamic_loader`)**: Manages the lifecycle of loaded skills.

For a deeper dive, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## 🤝 Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details on our code of conduct and the process for submitting pull requests.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---
*Built with ❤️ for the future of agentic computing.*
