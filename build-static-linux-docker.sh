#!/bin/bash
set -e

echo "Building i2pd-webconsole-exporter for x86_64 Linux..."
docker build -t i2pd-webconsole-exporter-build -f Dockerfile.build .

echo "Extracting binary..."
docker create --name temp-container i2pd-webconsole-exporter-build
mkdir -p ./dist
docker cp temp-container:/build/x86_64-unknown-linux-gnu/release/i2pd-webconsole-exporter ./dist/i2pd-webconsole-exporter
docker rm temp-container

echo "Build complete: ./dist/i2pd-webconsole-exporter"
file ./dist/i2pd-webconsole-exporter
