#!/bin/sh
# Generate ~10 GB of zero-filled dummy files for testing dutui.
# Usage: scripts/gen-example.sh [DIR]   (default: ./example)
# Remove it again with: rm -rf example
set -eu

root=${1:-example}

if [ -e "$root" ] && [ -n "$(ls -A "$root" 2>/dev/null)" ]; then
    echo "error: $root already exists and is not empty" >&2
    exit 1
fi

# f PATH SIZE_KIB: create a zero-filled file of SIZE_KIB KiB (real blocks, not sparse).
f() {
    mkdir -p "$(dirname "$root/$1")"
    head -c "$(($2 * 1024))" /dev/zero >"$root/$1"
}
mib() { f "$1" "$(($2 * 1024))"; }

# n PREFIX SUFFIX COUNT MIB: numbered files, e.g. n photos/a/IMG_ .jpg 3 8
n() {
    i=1
    while [ "$i" -le "$3" ]; do
        mib "$(printf '%s%03d%s' "$1" "$i" "$2")" "$4"
        i=$((i + 1))
    done
}

echo "generating $root ..."

# videos
mib videos/2023/summer-trip/clip-001.mov 1024
mib videos/2023/summer-trip/clip-002.mov 1024
mib videos/2023/wedding/ceremony.mp4 700
mib videos/2023/wedding/reception.mp4 600
mib videos/2024/conference/keynote.mp4 450
mib videos/2024/conference/panel.mp4 250

# backups
mib backups/laptop/time-machine-2024-01.sparsebundle 1500
mib backups/laptop/time-machine-2024-02.sparsebundle 800
mib backups/databases/prod-dump-2024-03.sql 300

# downloads
mib downloads/ubuntu-24.04.iso 700
mib downloads/installer.dmg 250
mib downloads/archives/dataset.zip 120
mib downloads/archives/old-site.tar.gz 40

# photos
n photos/raw/2024-05/IMG_ .raw 30 40
n photos/edited/2024-05/IMG_ .jpg 20 20
n photos/edited/2023/DSC_ .jpg 10 8

# hidden cache
mib .cache/build/artifacts.bin 100
mib .cache/thumbnails/store.db 200

# projects: many small files, like a node_modules tree
for p in api-server mobile-app webapp; do
    mib "projects/$p/dist/bundle.js" 25
    mib "projects/$p/src/main.ts" 5
    pkg=1
    while [ "$pkg" -le 12 ]; do
        file=1
        while [ "$file" -le 8 ]; do
            f "$(printf 'projects/%s/node_modules/pkg-%02d/lib/file%d.js' "$p" "$pkg" "$file")" 200
            file=$((file + 1))
        done
        pkg=$((pkg + 1))
    done
done

# documents
i=1
while [ "$i" -le 10 ]; do
    mib "documents/invoices/2024/invoice-$i.pdf" 1
    i=$((i + 1))
done
i=1
while [ "$i" -le 12 ]; do
    mib "documents/reports/report-$i.pdf" 2
    i=$((i + 1))
done
mib documents/presentations/q3-review.pptx 10

echo "done: $(du -sh "$root" | cut -f1) in $(find "$root" -type f | wc -l | tr -d ' ') files"
