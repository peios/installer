#!/bin/sh
# Runs on the host: boot the GXWI dev VM with disks in it, for the installer
# to have a machine to look at. It is ../gxwi/dev/boot.sh, which stays up
# until the VM is killed, with the disks dev/disks.sh makes attached each a
# different way, so that the disk page has every kind to show:
#
#   NVMe    windows.img   a Windows disk
#   SATA    data.img      files, and another Linux
#   SATA    release.img   a disk holding the release the medium carries
#   USB     blank.img     an empty stick, removable
#   virtio  peios.qcow2   a disk Peios was really installed on, if there is one
#
# and the medium it boots from, which is virtio too.
#
# It starts as a machine does, through the firmware (OVMF), and a restart is a
# restart rather than the end of the VM, so that what the installer installs
# can be restarted into. The stick comes first in the firmware's order and
# the medium last: an empty stick has nothing to start and the medium starts,
# and once something is installed on the stick, that starts.
#
#     dev/boot.sh             # in another terminal; it stays up
#     dev/push.sh             # build, and put it in the VM
#     dev/overlay.sh on       # http://127.0.0.1:7780/ is now the installer
#     node dev/browser/machine.mjs
set -eu
cd "$(dirname "$0")/.."
dev/disks.sh
d=$PWD/target/disks
# $1 is what QEMU is to call the drive, $2 its file, $3 the file's format.
drive() { printf ' -drive if=none,id=%s,format=%s,file=%s' "$1" "${3:-raw}" "$d/$2"; }
extra="-device ahci,id=sata -device qemu-xhci,id=usb"
extra="$extra$(drive windows windows.img) -device nvme,drive=windows,serial=installer-gxwi"
extra="$extra$(drive data data.img) -device ide-hd,drive=data,bus=sata.0,model=WDC-WD20EZAZ"
extra="$extra$(drive blank blank.img) -device usb-storage,drive=blank,bus=usb.0,removable=on,bootindex=1"
if [ -e "$d/release.img" ]; then
    extra="$extra$(drive release release.img) -device ide-hd,drive=release,bus=sata.1,model=CT500MX500SSD1"
fi
if [ -e "$d/peios.qcow2" ]; then
    extra="$extra$(drive peios peios.qcow2 qcow2) -device virtio-blk-pci,drive=peios"
fi
VM_EXTRA="$extra" DRIVE_ARGS="--uefi --reboot" exec ../gxwi/dev/boot.sh
