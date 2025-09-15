#!/bin/bash
# Test build script
echo "Testing Reaper build..."
echo "Building..."
cargo build --release --features cache --quiet

if [ $? -eq 0 ]; then
    echo "✅ Build successful!"
    
    echo "Checking for warnings..."
    WARNINGS=$(cargo build --release --features cache 2>&1 | grep -c "warning:")
    
    if [ "$WARNINGS" -eq 0 ]; then
        echo "✅ No warnings found!"
    else
        echo "⚠️  Found $WARNINGS warnings"
        echo "Running cargo build to show warnings..."
        cargo build --release --features cache
    fi
    
    echo ""
    echo "Running clippy check..."
    cargo clippy --release --features cache --quiet
    
    if [ $? -eq 0 ]; then
        echo "✅ Clippy check passed!"
    else
        echo "⚠️ Clippy found issues"
    fi
else
    echo "❌ Build failed!"
    echo "Running cargo build to show errors..."
    cargo build --release --features cache
fi

echo ""
echo "Build test completed!"