#!/bin/bash

rm  -rf dist/*
./build.sh
mkdir -p dist
cp -f install.sh out
cp -f cypress-display.service out
pushd out
zip -r ../dist/cypress-display.zip .
popd
