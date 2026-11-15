#!/usr/bin/env bash
# Cauce packaging gate (`docs/architecture.md` §16, `docs/backlog.md` G4,
# `odd/tasks/cauce-0.1.md` T21/C-051).
#
#   packaging/check-package.sh target/debian/cauce_0.1.0-1_amd64.deb
#
# Validates the built `.deb` without root and without installing anything: the
# archive is read with `dpkg-deb -I` and `dpkg-deb -c`, and extracted with
# `dpkg-deb -x` into a temporary directory so the AppStream and desktop
# validators run on the installed copies, not on the sources in `packaging/`.
#
# Every defect is printed as one concrete line; the exit status is 1 when there
# is at least one defect, 2 when the gate cannot even run, and 0 with
# `check-package: OK <path>` when the package is valid.
set -uo pipefail

readonly PROG=check-package

readonly EXPECTED_PACKAGE=cauce
# Exactly the runtime dependencies of §16, without `$auto` and without extras.
readonly EXPECTED_DEPENDS=(ffmpeg libgtk-4-1 libadwaita-1-0 libnotify4 xdg-desktop-portal)
# Every member the package must install: path (without the leading `./`) plus
# the octal mode §16 requires. Binaries 755, units and data 644.
readonly EXPECTED_MEMBERS=(
    "usr/bin/cauce-daemon 755"
    "usr/bin/cauce-gui 755"
    "usr/bin/cauce-cli 755"
    "usr/lib/systemd/user/cauce.socket 644"
    "usr/lib/systemd/user/cauce.service 644"
    "usr/share/applications/cauce.desktop 644"
    "usr/share/metainfo/io.github.wilkin.Cauce.metainfo.xml 644"
    "usr/share/icons/hicolor/scalable/apps/io.github.wilkin.Cauce.svg 644"
)

defects=()

defect() { defects+=("$*"); }

note() { printf '%s: %s\n' "$PROG" "$*" >&2; }

first_line() { printf '%s\n' "$1" | grep -v '^[[:space:]]*$' | head -n 1; }

# `-rwxr-xr-x` -> `755`, without trusting the archive's own octal output.
mode_octal() {
    local perms=$1 bits digits="" digit chunk i
    bits=${perms:1}
    for i in 0 1 2; do
        chunk=${bits:$((i * 3)):3}
        digit=0
        case ${chunk:0:1} in r) digit=$((digit + 4)) ;; esac
        case ${chunk:1:1} in w) digit=$((digit + 2)) ;; esac
        case ${chunk:2:1} in x | s | t) digit=$((digit + 1)) ;; esac
        digits+=$digit
    done
    printf '%s' "$digits"
}

# First field (the permission string) of a member in `dpkg-deb -c` output.
member_perms() {
    printf '%s\n' "$listing" | awk -v target="./$1" '$NF == target { print $1; exit }'
}

# ---------------------------------------------------------------- preconditions

if [ "$#" -ne 1 ]; then
    note "uso: $PROG <ruta-al-.deb>"
    exit 2
fi

deb=$1

if [ ! -e "$deb" ]; then
    note "no existe el paquete: $deb"
    exit 2
fi
if [ ! -f "$deb" ] || [ ! -r "$deb" ]; then
    note "el paquete no es un archivo legible: $deb"
    exit 2
fi

for tool in dpkg-deb appstreamcli desktop-file-validate; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        note "falta la herramienta $tool; el paquete no se puede validar"
        exit 2
    fi
done

work=$(mktemp -d "${TMPDIR:-/tmp}/check-package.XXXXXX") || {
    note "no se pudo crear un directorio temporal"
    exit 2
}
cleanup() {
    if [ -n "${work:-}" ] && [ -d "$work" ]; then
        rm -rf -- "$work"
    fi
}
trap cleanup EXIT

# ------------------------------------------------------- 1. control and Depends

if ! control=$(dpkg-deb -I "$deb" 2>&1); then
    note "dpkg-deb -I falló: $(first_line "$control")"
    exit 2
fi

package=$(printf '%s\n' "$control" | sed -n 's/^ *Package: *//p' | head -n 1)
if [ "$package" != "$EXPECTED_PACKAGE" ]; then
    defect "Package: se esperaba '$EXPECTED_PACKAGE' y el paquete declara '${package:-<vacío>}'"
fi

depends=$(printf '%s\n' "$control" | sed -n 's/^ *Depends: *//p' | head -n 1)
expected_depends=$(printf '%s\n' "${EXPECTED_DEPENDS[@]}" | LC_ALL=C sort | paste -sd ' ')
declared_depends=$(printf '%s\n' "$depends" |
    tr ',' '\n' | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' |
    grep -v '^$' | LC_ALL=C sort | paste -sd ' ')
if [ "$declared_depends" != "$expected_depends" ]; then
    case $depends in
        *'$auto'*)
            defect "Depends: declara \$auto, prohibido por §16; se esperaba '$expected_depends'"
            ;;
        *)
            defect "Depends: se esperaba '$expected_depends' y el paquete declara '${declared_depends:-<vacío>}'"
            ;;
    esac
fi

# ------------------------------------------------ 2. short description (L35)

description=$(printf '%s\n' "$control" | sed -n 's/^ *Description: *//p' | head -n 1)
case $description in
    *Cauce*) ;;
    *) defect "Description: la descripción corta no menciona Cauce: '${description:-<vacío>}'" ;;
esac
case $(printf '%s' "$description" | tr '[:upper:]' '[:lower:]') in
    *'headless daemon'*)
        defect "Description: la descripción corta dice 'headless daemon' y el paquete trae daemon, GUI y CLI: '$description'"
        ;;
esac

# ------------------------------------------------------------ 3. members/modes

if ! listing=$(dpkg-deb -c "$deb" 2>&1); then
    note "dpkg-deb -c falló: $(first_line "$listing")"
    exit 2
fi

for entry in "${EXPECTED_MEMBERS[@]}"; do
    read -r path expected_mode <<< "$entry"
    perms=$(member_perms "$path")
    if [ -z "$perms" ]; then
        defect "falta /$path en el paquete"
        continue
    fi
    case $perms in
        d*) defect "/$path está en el paquete como directorio ($perms)" ;;
        *)
            got_mode=$(mode_octal "$perms")
            if [ "$got_mode" != "$expected_mode" ]; then
                defect "/$path tiene modo $got_mode ($perms) y debe ser $expected_mode"
            fi
            ;;
    esac
done

# --------------------------------------- 4. validators on the installed copies

root="$work/root"
if ! extract=$(dpkg-deb -x "$deb" "$root" 2>&1); then
    defect "dpkg-deb -x falló: $(first_line "$extract")"
else
    metainfo="$root/usr/share/metainfo/io.github.wilkin.Cauce.metainfo.xml"
    if [ -f "$metainfo" ]; then
        if ! out=$(appstreamcli validate --no-net "$metainfo" 2>&1); then
            defect "appstreamcli validate falló: $(first_line "$out")"
        fi
    fi

    desktop="$root/usr/share/applications/cauce.desktop"
    if [ -f "$desktop" ]; then
        if ! out=$(desktop-file-validate "$desktop" 2>&1); then
            defect "desktop-file-validate falló: $(first_line "$out")"
        fi
    fi

    # ------------------------------------------------- 5. unit file directives
    socket_unit="$root/usr/lib/systemd/user/cauce.socket"
    if [ -f "$socket_unit" ]; then
        for directive in SocketMode=0600 DirectoryMode=0700; do
            if ! grep -qx -- "$directive" "$socket_unit"; then
                defect "cauce.socket no declara $directive"
            fi
        done
    fi

    service_unit="$root/usr/lib/systemd/user/cauce.service"
    if [ -f "$service_unit" ]; then
        if ! grep -qx -- "ExecStart=/usr/bin/cauce-daemon" "$service_unit"; then
            defect "cauce.service no declara ExecStart=/usr/bin/cauce-daemon"
        fi
    fi
fi

# ----------------------------------------------------------------- 6. verdict

if [ "${#defects[@]}" -gt 0 ]; then
    for item in "${defects[@]}"; do
        note "$item"
    done
    note "FALLÓ $deb (${#defects[@]} defecto(s))"
    exit 1
fi

printf '%s: OK %s\n' "$PROG" "$deb"
