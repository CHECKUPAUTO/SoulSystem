#!/bin/bash
cd /root/soul_system
cargo check 2>&1
echo "EXIT=$?"
