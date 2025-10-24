#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGING_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PROJECT_ROOT="$(cd "${PACKAGING_ROOT}/.." && pwd)"
DIST_DIR="${PROJECT_ROOT}/dist"
BUILD_ROOT="${SCRIPT_DIR}/build"
STAGING_DIR="${BUILD_ROOT}/harmoniq-studio"
DEBIAN_DIR="${STAGING_DIR}/DEBIAN"
SHARED_DIR="${PACKAGING_ROOT}/shared"

if ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "dpkg-deb is required but was not found in PATH" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required but was not found in PATH" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required but was not found in PATH" >&2
  exit 1
fi

mkdir -p "${DIST_DIR}"
rm -rf "${BUILD_ROOT}"
mkdir -p "${DEBIAN_DIR}" "${STAGING_DIR}/usr/bin" \
         "${STAGING_DIR}/usr/share/applications" \
         "${STAGING_DIR}/usr/share/icons/hicolor/scalable/apps" \
         "${STAGING_DIR}/usr/share/doc/harmoniq-studio"

pushd "${PROJECT_ROOT}" >/dev/null
cargo build --release -p harmoniq-app
popd >/dev/null

install -Dm755 "${PROJECT_ROOT}/target/release/harmoniq-app" "${STAGING_DIR}/usr/bin/harmoniq-studio"
install -Dm644 "${SHARED_DIR}/harmoniq-studio.desktop" "${STAGING_DIR}/usr/share/applications/harmoniq-studio.desktop"
install -Dm644 "${PROJECT_ROOT}/resources/icons/harmoniq-studio.svg" "${STAGING_DIR}/usr/share/icons/hicolor/scalable/apps/harmoniq-studio.svg"
install -Dm644 "${PROJECT_ROOT}/README.md" "${STAGING_DIR}/usr/share/doc/harmoniq-studio/README.md"

VERSION=$(cargo metadata --format-version 1 --no-deps --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
  | jq -r '.packages[] | select(.name=="harmoniq-app") | .version')

if [ -z "${VERSION}" ] || [ "${VERSION}" = "null" ]; then
  echo "Failed to determine harmoniq-app version" >&2
  exit 1
fi

if command -v dpkg-architecture >/dev/null 2>&1; then
  ARCH=$(dpkg-architecture -qDEB_HOST_ARCH)
elif command -v dpkg >/dev/null 2>&1; then
  ARCH=$(dpkg --print-architecture)
else
  case "$(uname -m)" in
    x86_64) ARCH="amd64" ;;
    aarch64) ARCH="arm64" ;;
    armv7l|armv7hf|armhf) ARCH="armhf" ;;
    i686|i386) ARCH="i386" ;;
    *) ARCH="$(uname -m)" ;;
  esac
fi

if [ -n "${DEB_ARCH:-}" ]; then
  ARCH="${DEB_ARCH}"
fi

DEPENDS=""
if command -v dpkg-shlibdeps >/dev/null 2>&1; then
  DEPENDS=$(dpkg-shlibdeps -O "${STAGING_DIR}/usr/bin/harmoniq-studio" 2>/dev/null || true)
  DEPENDS=$(printf '%s' "${DEPENDS}" | sed -n 's/^shlibs:Depends=//p')
  DEPENDS=$(printf '%s' "${DEPENDS}" | tr -d '\n')
else
  echo "Warning: dpkg-shlibdeps not found; dependency metadata will be omitted." >&2
fi

CONTROL_FILE="${DEBIAN_DIR}/control"
{
  echo "Package: harmoniq-studio"
  echo "Version: ${VERSION}"
  echo "Section: sound"
  echo "Priority: optional"
  echo "Architecture: ${ARCH}"
  echo "Maintainer: Harmoniq Studio Developers <maintainers@harmoniq.studio>"
  if [ -n "${DEPENDS}" ]; then
    echo "Depends: ${DEPENDS}"
  fi
  echo "Homepage: https://harmoniq.studio"
  echo "Description: Native digital audio workstation"
  echo " Harmoniq Studio is a native, multi-platform digital audio workstation"
  echo " focused on expressive performance workflows."
} > "${CONTROL_FILE}"

chmod 755 "${DEBIAN_DIR}"
find "${STAGING_DIR}" -type d -print0 | xargs -0 chmod 755
find "${STAGING_DIR}" -type f -print0 | xargs -0 chmod go-w

PACKAGE_NAME="harmoniq-studio_${VERSION}_${ARCH}.deb"

dpkg-deb --build "${STAGING_DIR}" "${DIST_DIR}/${PACKAGE_NAME}"

echo "Created ${DIST_DIR}/${PACKAGE_NAME}"

rm -rf "${BUILD_ROOT}"
