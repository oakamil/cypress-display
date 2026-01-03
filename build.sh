#!/bin/bash

cargo build --release

mkdir -p bin
cp -f target/release/cypress-display bin
