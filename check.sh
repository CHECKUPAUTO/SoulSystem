#!/bin/bash
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cargo check 2>&1
echo "EXIT=$?"
