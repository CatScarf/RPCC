VALUE=$(grep -m1 '^name =' Cargo.toml | awk -F '"' '{print $2}')
echo "APP_NAME=$VALUE" >> "$GITHUB_OUTPUT"

VALUE=$(grep -m1 '^version =' Cargo.toml | awk -F '"' '{print $2}')
echo "APP_VERSION=$VALUE" >> "$GITHUB_OUTPUT"

VALUE=$(date "+%Y%m%d_%H%M%S")
echo "APP_BUILD_TIME=$VALUE" >> "$GITHUB_OUTPUT"

echo "cat $GITHUB_OUTPUT"
cat "$GITHUB_OUTPUT"