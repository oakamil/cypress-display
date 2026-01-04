#!/bin/bash

cargo build --release

rm -rf out
mkdir -p out/cypress/bin
cp -f target/release/cypress-display out/cypress/bin
cp -rf web out/cypress/bin
