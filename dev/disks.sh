#!/bin/sh
# Runs on the host: make the disks the dev VM is given for the disk page to
# have something to say, under target/disks. dev/boot.sh attaches them.
#
#   windows.img   a machine's Windows disk: an EFI system partition holding
#                 Windows' boot manager, the reserved partition, an NTFS
#                 system partition with files on it, and a recovery partition
#   data.img      an exFAT partition of files, and an ext4 one holding another
#                 Linux, whose journal says it wants replaying and whose
#                 /etc/os-release is a link pointing out of the disk
#   blank.img     nothing at all: somewhere to install
#   release.img   a disk as an install leaves it, as far as saying which
#                 release it holds goes: the os-release and the package
#                 database of the newest image built here, and nothing else
#   peios.qcow2   ../dist/release/disk.img as it is, if something has been
#                 installed onto it, behind a layer that takes the writes. A
#                 real install, which may be too old to say its release.
#
# Nothing here needs root: each filesystem is made in a file of its own and
# copied to where its partition is. It wants sgdisk, mkfs.vfat, mtools,
# mkntfs and ntfs-3g (with FUSE), mkfs.exfat, mke2fs, debugfs and qemu-img.
#
# Each is made once: delete one to have it made again, or target/disks for
# them all. Not while a VM has them.
set -eu
cd "$(dirname "$0")/.."
out=target/disks
w=$out/work
rm -rf "$w"
mkdir -p "$w"

# $1 megabytes of something that is not nothing, at $2.
filler() { head -c "$(($1 * 1048576))" /dev/urandom > "$2"; }
# Copies filesystem image $1 into disk $2 at megabyte $3, holes kept as holes.
place() { dd if="$1" of="$2" bs=1M seek="$3" conv=notrunc,sparse status=none; }
# Where sgdisk put partition $2 of disk $1, and how big, in megabytes.
at() { sgdisk -i "$2" "$1" | sed -n 's/^First sector: \([0-9]*\) .*/\1/p' | { read -r s; echo $((s / 2048)); }; }
mib() { sgdisk -i "$2" "$1" | sed -n 's/^Partition size: \([0-9]*\) sectors.*/\1/p' | { read -r s; echo $((s / 2048)); }; }
# A disk is made under another name and given its own when it is whole.
done_with() { mv "$out/$1.part" "$out/$1"; }

if [ ! -e "$out/windows.img" ]; then
    disk=$out/windows.img.part
    rm -f "$disk"
    truncate -s 4G "$disk"
    sgdisk \
        -n 1:2048:+100M -t 1:ef00 -c 1:"EFI system partition" \
        -n 2:0:+16M -t 2:0c01 -c 2:"Microsoft reserved partition" \
        -n 3:0:+3400M -t 3:0700 -c 3:"Basic data partition" \
        -n 4:0:0 -t 4:2700 -c 4:"" \
        "$disk" > /dev/null

    truncate -s 100M "$w/esp.img"
    mkfs.vfat -F 32 "$w/esp.img" > /dev/null
    echo bootmgfw > "$w/bootmgfw.efi"
    mmd -i "$w/esp.img" ::EFI ::EFI/Microsoft ::EFI/Microsoft/Boot ::EFI/Boot
    mcopy -i "$w/esp.img" "$w/bootmgfw.efi" ::EFI/Microsoft/Boot/bootmgfw.efi
    mcopy -i "$w/esp.img" "$w/bootmgfw.efi" ::EFI/Boot/bootx64.efi
    place "$w/esp.img" "$disk" "$(at "$disk" 1)"

    truncate -s "$(mib "$disk" 3)M" "$w/system.img"
    mkntfs -F -Q -q -L Windows "$w/system.img" > /dev/null 2>&1
    mkdir -p "$w/mnt"
    ntfs-3g "$w/system.img" "$w/mnt"
    mkdir -p "$w/mnt/Windows/System32" "$w/mnt/Users/sam/Documents"
    echo ntoskrnl > "$w/mnt/Windows/System32/ntoskrnl.exe"
    filler 600 "$w/mnt/Users/sam/Documents/thesis.bin"
    fusermount -u "$w/mnt"
    place "$w/system.img" "$disk" "$(at "$disk" 3)"

    truncate -s "$(($(mib "$disk" 4) - 1))M" "$w/recovery.img"
    mkntfs -F -Q -q "$w/recovery.img" > /dev/null 2>&1
    place "$w/recovery.img" "$disk" "$(at "$disk" 4)"
    done_with windows.img
fi

if [ ! -e "$out/data.img" ]; then
    disk=$out/data.img.part
    rm -f "$disk"
    truncate -s 3G "$disk"
    sgdisk \
        -n 1:2048:+1500M -t 1:0700 -c 1:"Basic data partition" \
        -n 2:0:0 -t 2:8300 -c 2:"" \
        "$disk" > /dev/null
    truncate -s 1500M "$w/media.img"
    mkfs.exfat -L MEDIA "$w/media.img" > /dev/null
    place "$w/media.img" "$disk" "$(at "$disk" 1)"

    mkdir -p "$w/linux/usr/lib" "$w/linux/etc" "$w/linux/home/sam"
    printf 'NAME="Debian GNU/Linux"\nPRETTY_NAME="Debian GNU/Linux 13 (trixie)"\nID=debian\n' > "$w/linux/usr/lib/os-release"
    # Absolute, as nothing stops a disk's maker making it: followed from the
    # installer it would be the medium's own os-release.
    ln -s /usr/lib/os-release "$w/linux/etc/os-release"
    filler 200 "$w/linux/home/sam/photos.bin"
    truncate -s "$(($(mib "$disk" 2) - 1))M" "$w/linux.img"
    mke2fs -q -t ext4 -L home -d "$w/linux" "$w/linux.img"
    # As a machine leaves it when the power goes: the journal wants replaying.
    debugfs -w -R "feature needs_recovery" "$w/linux.img" > /dev/null 2>&1
    place "$w/linux.img" "$disk" "$(at "$disk" 2)"
    done_with data.img
fi

if [ ! -e "$out/blank.img" ]; then
    truncate -s 8G "$out/blank.img"
fi

# The newest image built here, the way ../dist/release/drive.py finds the one
# it boots: the release on this disk is then the release on the medium.
built=$(ls -dt ../dist/release/dist/peios-experimental-[0-9]* ../dist/peios-experimental-[0-9]* 2> /dev/null | grep -v -- '-dwe$' | head -n 1 || true)
if [ ! -e "$out/release.img" ] && [ -e "$built/root/var/state/peipkg/db.sqlite" ]; then
    disk=$out/release.img.part
    rm -f "$disk"
    truncate -s 2G "$disk"
    sgdisk \
        -n 1:2048:+512M -t 1:ef00 -c 1:"EFI system partition" \
        -n 2:0:0 -t 2:8300 -c 2:"Peios root" \
        "$disk" > /dev/null
    truncate -s 512M "$w/boot.img"
    mkfs.vfat -F 32 -n PEIOSESP "$w/boot.img" > /dev/null
    place "$w/boot.img" "$disk" "$(at "$disk" 1)"
    mkdir -p "$w/peios/usr/lib" "$w/peios/var/state/peipkg"
    cp "$built/root/usr/lib/os-release" "$w/peios/usr/lib/os-release"
    cp "$built/root/var/state/peipkg/db.sqlite" "$w/peios/var/state/peipkg/db.sqlite"
    truncate -s "$(($(mib "$disk" 2) - 1))M" "$w/root.img"
    mke2fs -q -t ext4 -L peios-root -d "$w/peios" "$w/root.img"
    place "$w/root.img" "$disk" "$(at "$disk" 2)"
    done_with release.img
fi

installed=../dist/release/disk.img
if [ ! -e "$out/peios.qcow2" ] && [ -e "$installed" ] && sgdisk -p "$installed" 2> /dev/null | grep -q 'EF00'; then
    qemu-img create -q -f qcow2 -F raw -b "$(realpath "$installed")" "$out/peios.qcow2"
fi

rm -rf "$w"
rm -f "$out/made"
ls -l "$out"
