#!/bin/sh
# Boot an explicitly supplied development image with disposable fixture disks.
# VM_RUNNER is an executable accepting IMAGE followed by individual QEMU arguments;
# it owns firmware, networking and boot-medium configuration. It must keep guest
# writes off the supplied image and support guest reboot. No image is discovered.
set -eu
: "${IMAGE:?set IMAGE to the development image or composed image directory}"
: "${VM_RUNNER:?set VM_RUNNER to an executable development VM runner}"
image=$(realpath "$IMAGE")
runner=$(realpath "$VM_RUNNER")
[ -e "$image" ] || {
    echo "IMAGE does not exist: $image" >&2
    exit 1
}
[ -x "$runner" ] || {
    echo "VM_RUNNER is not executable: $runner" >&2
    exit 1
}
cd "$(dirname "$0")/.."
dev/disks.sh
d=$PWD/target/disks
# QEMU's key/value syntax uses doubled commas inside filenames.
escape_path() { printf '%s' "$1" | sed 's/,/,,/g'; }
set -- -device ahci,id=sata -device qemu-xhci,id=usb -drive "if=none,id=windows,format=raw,file=$(escape_path "$d/windows.img")" -device nvme,drive=windows,serial=installer-gxwi -drive "if=none,id=data,format=raw,file=$(escape_path "$d/data.img")" -device ide-hd,drive=data,bus=sata.0,model=WDC-WD20EZAZ -drive "if=none,id=blank,format=raw,file=$(escape_path "$d/blank.img")" -device usb-storage,drive=blank,bus=usb.0,removable=on,bootindex=1
if [ -e "$d/release.img" ]; then
    set -- "$@" -drive "if=none,id=release,format=raw,file=$(escape_path "$d/release.img")" -device ide-hd,drive=release,bus=sata.1,model=CT500MX500SSD1
fi
if [ -e "$d/peios.qcow2" ]; then
    set -- "$@" -drive "if=none,id=peios,format=qcow2,file=$(escape_path "$d/peios.qcow2")" -device virtio-blk-pci,drive=peios
fi
exec "$runner" "$image" "$@"
