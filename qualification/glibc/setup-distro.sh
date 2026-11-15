#!/bin/sh
# Inside an old-distribution container (as root): point end-of-life package
# repositories at their archives, then install Xvfb, the X utilities the
# launch test uses, and the window's graphics runtime. Packages that a
# distribution doesn't have are skipped and listed. Writes facts about the
# userland to $1 (key=value lines). CI-only; see .github/workflows/glibc-experiments.yml.
set -u
facts=$1
note() { echo "$1=$2" >> "$facts"; }
id=unknown; ver=
if [ -f /etc/os-release ]; then . /etc/os-release; id=$ID; ver=${VERSION_ID:-}; fi
if [ -f /etc/centos-release ] && grep -q 'release 6' /etc/centos-release; then id=centos; ver=6; fi
note distro "$id $ver"
missing=
each() { # install packages one at a time; keep going when one is missing
  for p in "$@"; do $install "$p" >/dev/null 2>&1 || missing="$missing $p"; done
}
case "$id" in
  centos)
    case "$ver" in 6) rel=6.10 ;; 7) rel=7.9.2009 ;; *) rel=$ver ;; esac
    sed -i -e 's|^mirrorlist=|#mirrorlist=|' \
      -e "s|^#\?baseurl=http://mirror.centos.org/centos/\$releasever|baseurl=http://vault.centos.org/$rel|" /etc/yum.repos.d/*.repo
    install="yum install -y"
    each xorg-x11-server-Xvfb xorg-x11-utils mesa-dri-drivers mesa-libEGL mesa-libGL libxkbcommon-x11 libXcursor libXrandr libXi vulkan-loader vulkan mesa-vulkan-drivers
    note mesa "$(rpm -q mesa-dri-drivers 2>&1 | head -1)" ;;
  almalinux|rocky|rhel)
    install="dnf install -y"
    each xorg-x11-server-Xvfb xorg-x11-utils mesa-dri-drivers mesa-libEGL libxkbcommon-x11 libXcursor libXrandr libXi vulkan-loader mesa-vulkan-drivers
    note mesa "$(rpm -q mesa-dri-drivers 2>&1 | head -1)" ;;
  ubuntu|debian)
    export DEBIAN_FRONTEND=noninteractive
    if [ "$id" = debian ] && [ "${ver%%.*}" -le 9 ] 2>/dev/null; then
      code=$(sed -n 's/^VERSION_CODENAME=//p' /etc/os-release)
      [ -n "$code" ] || code=$(sed -n 's/.*(\(.*\)).*/\1/p' /etc/os-release | head -1)
      echo "deb http://archive.debian.org/debian $code main" > /etc/apt/sources.list
      rm -f /etc/apt/sources.list.d/*
      opts="-o Acquire::Check-Valid-Until=false -o APT::Get::AllowUnauthenticated=true"
    else
      opts=
    fi
    apt-get $opts update >/dev/null 2>&1 || note apt_update failed
    install="apt-get $opts install -y --no-install-recommends"
    each xvfb x11-utils libxkbcommon-x11-0 libvulkan1 mesa-vulkan-drivers libegl1 libegl1-mesa libgl1-mesa-dri libxcursor1 libxrandr2 libxi6 libx11-xcb1
    note mesa "$(dpkg-query -W -f '${Package} ${Version}' libgl1-mesa-dri 2>&1 | head -1)" ;;
  opensuse|opensuse-leap)
    for r in $(zypper -q lr 2>/dev/null | awk -F'|' 'NR>2 {print $2}'); do zypper -q rr "$r" >/dev/null 2>&1; done
    zypper -q ar -f "http://ftp.gwdg.de/pub/opensuse/discontinued/distribution/leap/$ver/repo/oss/" oss >/dev/null 2>&1
    install="zypper -q --non-interactive --no-gpg-checks in"
    each xorg-x11-server xdpyinfo xwininfo Mesa-dri Mesa-libEGL1 libxkbcommon-x11-0 libXcursor1 libXrandr2 libXi6 libvulkan1 Mesa-libvulkan
    note mesa "$(rpm -q Mesa-dri 2>&1 | head -1)" ;;
  *) note setup "unknown distribution" ;;
esac
note missing_packages "${missing# }"
note glibc "$(ldd --version 2>&1 | sed -n 1p)"
for t in Xvfb xdpyinfo xwininfo python3; do command -v $t >/dev/null 2>&1 || note "no_$t" yes; done
# No system fonts: the app's fonts are built in.
rm -rf /usr/share/fonts /usr/local/share/fonts /etc/fonts
exit 0
