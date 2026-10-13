#!/bin/sh
set -eu

root=/build/haiku-runtime-root
binary=/build/target/release/aether-gateway
test -f "$binary"
test ! -e "$root"
mkdir -p "$root/opt/aether/releases/image/bin"
install -m 0755 "$binary" "$root/opt/aether/releases/image/bin/aether-gateway"

# Match the project's GNU runtime packaging: recursively include the loader
# and shared libraries, while reusing production's frontend and configuration.
list=$(mktemp)
queue=$(mktemp)
trap 'rm -f "$list" "$queue"' EXIT
printf '%s\n' "$binary" > "$queue"
while [ -s "$queue" ]; do
    current=$(head -n 1 "$queue")
    sed -i '1d' "$queue"
    ldd "$current" | awk '/=> \/ / {print $3} /=> \// {print $3} $1 ~ /^\// {print $1}' |
    while read -r lib; do
        if [ -f "$lib" ] && ! grep -Fxq "$lib" "$list"; then
            printf '%s\n' "$lib" >> "$list"
            printf '%s\n' "$lib" >> "$queue"
        fi
    done
done
for lib in /lib/x86_64-linux-gnu/libnss_dns.so.2 /lib/x86_64-linux-gnu/libnss_files.so.2 /lib/x86_64-linux-gnu/libresolv.so.2; do
    if [ -f "$lib" ] && ! grep -Fxq "$lib" "$list"; then printf '%s\n' "$lib" >> "$list"; fi
done
sort -u "$list" -o "$list"
while read -r lib; do
    mkdir -p "$root$(dirname "$lib")"
    cp -L "$lib" "$root$lib"
done < "$list"
tar -czf /build/haiku-runtime.tar.gz -C "$root" .
sha256sum "$binary" /build/haiku-runtime.tar.gz
