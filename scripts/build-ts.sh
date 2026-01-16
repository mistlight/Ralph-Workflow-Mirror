#!/bin/bash

# TypeScript Build Script
# Compiles TypeScript files to JavaScript

set -e

echo "🔨 Building TypeScript..."

# Check if TypeScript is installed
if ! command -v tsc &> /dev/null; then
    echo "❌ TypeScript is not installed. Installing..."
    npm install -g typescript
fi

# Run TypeScript compiler
echo "📦 Compiling TypeScript files..."
npx tsc

# Check if compilation was successful
if [ $? -eq 0 ]; then
    echo "✅ TypeScript compilation successful!"
    echo "📝 Output: dist/scripts/"
else
    echo "❌ TypeScript compilation failed!"
    exit 1
fi
