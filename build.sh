#!/bin/bash

cargo build --release

rm  -rf dist/*
mkdir -p out/cypress/bin
mkdir -p dist
cp -f target/release/cypress-display out/cypress/bin
cp -f install.sh out
cp -f cypress-display.service out
pushd out
zip -r ../dist/cypress-display.zip .
popd
rm -rf out
