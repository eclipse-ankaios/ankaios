#!/usr/bin/env bash
set -euo pipefail

# Ankaios System Info collector
# Works in devcontainers, WSL2, or bare Linux. No external deps required.

# -----------------------
# Helpers
# -----------------------
have() { command -v "$1" >/dev/null 2>&1; }
val_or() { [ -n "${1-}" ] && printf '%s\n' "$1" || printf '%s\n' "$2"; }
read_os_release() {
  if [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    printf '%s\n' "${PRETTY_NAME:-$ID $VERSION_ID}"
  else
    printf 'Unknown (no /etc/os-release)\n'
  fi
}
kv() {
  # key-value pretty print
  printf '%s: %s\n' "$1" "$2"
}
try_ver() {
  local label="$1" bin="$2" args="${3-}"
  if have "$bin"; then
    local out=""
    if [ -n "$args" ]; then
      out="$("$bin" $args 2>/dev/null | head -n 1 || true)"
    else
      out="$("$bin" --version 2>/dev/null | head -n 1 || "$bin" -v 2>/dev/null | head -n 1 || true)"
    fi
    [ -z "$out" ] && out="installed (version unknown)"
    kv "$label" "$out"
  else
    kv "$label" "not found"
  fi
}
hr() { printf '%s\n' "----------------------------------------"; }

usage() {
  cat <<'EOF'
The script is used to get system information useful to the Ankaios project. It contains:
- Basic environment detection (container, WSL, cgroup mode)
- OS, kernel, architecture
- Versions of container runtimes and related tools
- Hardware details (CPU, memory, disk, GPU)
It does not contain sensitive information like hostnames or IPs.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --help|-h)
      usage
      exit 0
      ;;
    -*)
      echo "Error: unknown option '$1'" >&2
      usage
      exit 1
      ;;
    *)
      echo "Error: unexpected argument '$1'" >&2
      usage
      exit 1
      ;;
  esac
done

# -----------------------
# Environment Detection
# -----------------------
IN_CONTAINER="no"
CONTAINER_RUNTIME="unknown"
IN_DEVCONTAINER="no"
IN_WSL="no"
WSL_VERSION=""

# container heuristics
if [ -f "/.dockerenv" ] || [ -f "/run/.containerenv" ]; then
  IN_CONTAINER="yes"
fi

if have systemd-detect-virt; then
  if systemd-detect-virt --quiet --container; then
    IN_CONTAINER="yes"
  fi
fi

# cgroup inspection (works even without systemd)
if grep -E -q '(docker|podman|containerd|kubepods)' /proc/1/cgroup 2>/dev/null; then
  IN_CONTAINER="yes"
fi

# try to guess runtime (best-effort)
if grep -q 'docker' /proc/1/cgroup 2>/dev/null; then CONTAINER_RUNTIME="docker"; fi
if grep -q 'libpod' /proc/1/cgroup 2>/dev/null; then CONTAINER_RUNTIME="podman"; fi
if grep -q 'containerd' /proc/1/cgroup 2>/dev/null; then CONTAINER_RUNTIME="containerd"; fi

# devcontainer heuristics
# Common markers: VS Code Remote-Containers / Codespaces
if [ -n "${REMOTE_CONTAINERS-}" ] || [ -n "${DEVCONTAINER-}" ] || [ -n "${VSCODE_REMOTE_CONTAINERS-}" ]; then
  IN_DEVCONTAINER="yes"
fi
# Codespaces
if [ -n "${CODESPACES-}" ]; then IN_DEVCONTAINER="yes"; fi
# Typical devcontainer FS layout
if [ -d "/workspaces" ] || [ -d "/workspace" ]; then IN_DEVCONTAINER="yes"; fi

# WSL detection
if grep -i -q 'microsoft' /proc/version 2>/dev/null || [ -n "${WSL_DISTRO_NAME-}" ]; then
  IN_WSL="yes"
  # kernel string often contains microsoft-standard-WSL2
  WSL_VERSION="$(uname -r | sed -n 's/.*\(microsoft[^ ]*\).*/\1/p')"
fi

# cgroup mode
CGROUP_MODE="unknown"
if [ -d /sys/fs/cgroup ]; then
  # %T prints fstype; "cgroup2fs" means v2 unified
  if stat -f -c %T /sys/fs/cgroup 2>/dev/null | grep -q 'cgroup2fs'; then
    CGROUP_MODE="v2"
  else
    CGROUP_MODE="v1"
  fi
fi

# -----------------------
# Section: System
# -----------------------
hr
echo "Ankaios System Report"
hr
kv "Inside container" "$IN_CONTAINER"
kv "Container runtime (heuristic)" "$CONTAINER_RUNTIME"
kv "Inside devcontainer (heuristic)" "$IN_DEVCONTAINER"
kv "Inside WSL" "$IN_WSL"
[ "$IN_WSL" = "yes" ] && kv "WSL kernel tag" "$(val_or "$WSL_VERSION" "n/a")"

kv "OS" "$(read_os_release)"
kv "Kernel" "$(uname -r)"
kv "Architecture" "$(uname -m)"
if have ldd; then
  kv "libc (ldd)" "$(ldd --version 2>/dev/null | head -n1 | sed 's/\t/ /g')"
fi
kv "cgroups" "$CGROUP_MODE"

# -----------------------
# Section: Tool Versions
# -----------------------
hr
echo "Container & Tooling"
hr
try_ver "Podman" podman
try_ver "nerdctl" nerdctl
try_ver "Docker CLI" docker
try_ver "containerd" containerd "version"
try_ver "crictl" crictl "version"
try_ver "runc" runc
try_ver "crun" crun
try_ver "buildah" buildah

# Helpful language toolchains
hr
echo "Language Toolchains (if present)"
hr
try_ver "Rust (rustc)" rustc
try_ver "Cargo" cargo
try_ver "Python" python3 "--version"
try_ver "Node.js" node "--version"

# -----------------------
# Section: Hardware & Resources
# -----------------------
hr
echo "Hardware & Resources"
hr

# CPU
if have lscpu; then
  kv "CPU Model" "$(lscpu 2>/dev/null | awk -F: '/Model name/ {gsub(/^[ \t]+/,"",$2); print $2; exit}')"
  kv "CPU(s)" "$(lscpu 2>/dev/null | awk -F: '/^CPU\(s\)/ {gsub(/^[ \t]+/,"",$2); print $2; exit}')"
else
  kv "CPU Model" "$(awk -F: '/model name/ {print $2; exit}' /proc/cpuinfo 2>/dev/null | sed 's/^ //')"
  kv "CPU(s)" "$(grep -c '^processor' /proc/cpuinfo 2>/dev/null || echo unknown)"
fi

# Memory
if have free; then
  kv "Memory (total)" "$(free -h 2>/dev/null | awk '/Mem:/ {print $2}')"
  kv "Memory (available)" "$(free -h 2>/dev/null | awk '/Mem:/ {print $7}')"
else
  kv "Memory (kB MemTotal)" "$(awk '/MemTotal/ {print $2" kB"}' /proc/meminfo 2>/dev/null)"
fi

# Disk root
if have df; then
  kv "Disk root (/)" "$(df -h / 2>/dev/null | awk 'NR==2 {print $2" total, " $4" free"}')"
fi

# GPU (optional)
if have nvidia-smi; then
  kv "NVIDIA GPU" "$(nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null | paste -sd '; ' -)"
else
  kv "NVIDIA GPU" "nvidia-smi not found"
fi

# cgroup quotas (useful in containers)
if [ -r /sys/fs/cgroup/cpu.max ]; then
  kv "CPU cgroup (cpu.max)" "$(cat /sys/fs/cgroup/cpu.max)"
fi
if [ -r /sys/fs/cgroup/memory.max ]; then
  kv "Mem cgroup (memory.max)" "$(cat /sys/fs/cgroup/memory.max)"
fi

hr
echo "Done."
