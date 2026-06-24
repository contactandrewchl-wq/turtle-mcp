#!/usr/bin/env sh
# Instalador de Turtle para Linux y macOS — un solo comando, sin prerrequisitos.
#
#   curl -fsSL https://raw.githubusercontent.com/contactandrewchl-wq/turtle-mcp/main/install.sh | sh
#
# Descarga el binario ya compilado del último GitHub Release y lo deja en el PATH.
# No requiere Rust ni compilador C: solo curl, que el sistema ya trae.
#
# Repo privado (durante las pruebas): exporta primero un token con permiso de lectura, p. ej.
#   export GITHUB_TOKEN="$(gh auth token)"
# Opcionales:  TURTLE_VERSION=v0.1.0   TURTLE_INSTALL_DIR=$HOME/.local/bin
set -eu

repo="contactandrewchl-wq/turtle-mcp"
install_dir="${TURTLE_INSTALL_DIR:-$HOME/.local/bin}"
version="${TURTLE_VERSION:-}"
token="${GITHUB_TOKEN:-${GH_TOKEN:-}}"

os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
    Linux) plat="unknown-linux-musl" ;;
    Darwin) plat="apple-darwin" ;;
    *) echo "turtle: sistema operativo no soportado: $os" >&2; exit 1 ;;
esac
case "$arch" in
    x86_64 | amd64) cpu="x86_64" ;;
    arm64 | aarch64) cpu="aarch64" ;;
    *) echo "turtle: arquitectura no soportada: $arch" >&2; exit 1 ;;
esac
target="${cpu}-${plat}"
asset="turtle-${target}.tar.gz"

api="https://api.github.com/repos/${repo}/releases/latest"
[ -n "$version" ] && api="https://api.github.com/repos/${repo}/releases/tags/${version}"

auth_header=""
[ -n "$token" ] && auth_header="Authorization: Bearer $token"

echo "Buscando la release de ${repo}..."
rel="$(curl -fsSL -H 'User-Agent: turtle-installer' ${auth_header:+-H "$auth_header"} "$api")"

tag="$(printf '%s\n' "$rel" | grep -m1 '"tag_name":' | sed 's/.*"tag_name": *"//;s/".*//')"
[ -n "$tag" ] || { echo "turtle: no se pudo resolver la release." >&2; exit 1; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
archive="$tmp/$asset"

echo "Descargando turtle ${tag} (${target})..."
if [ -n "$token" ]; then
    # Repo privado: descargar por la API de assets (el id del asset precede a su nombre en el JSON).
    asset_id="$(printf '%s\n' "$rel" | awk -v want="\"name\": \"$asset\"" '
        /"id":/ { gsub(/[^0-9]/, "", $0); id=$0 }
        index($0, want) { print id; exit }')"
    [ -n "$asset_id" ] || { echo "turtle: la release ${tag} no incluye ${asset}." >&2; exit 1; }
    curl -fsSL -H "Authorization: Bearer $token" -H 'Accept: application/octet-stream' \
        "https://api.github.com/repos/${repo}/releases/assets/${asset_id}" -o "$archive"
else
    curl -fsSL "https://github.com/${repo}/releases/download/${tag}/${asset}" -o "$archive"
fi

tar -xzf "$archive" -C "$tmp"
mkdir -p "$install_dir"
install -m 0755 "$tmp/turtle-${target}/turtle" "$install_dir/turtle"

echo ""
echo "Turtle instalado en ${install_dir}/turtle."
case ":${PATH}:" in
    *":${install_dir}:"*) echo "Verifica con:  turtle --version" ;;
    *) echo "Agrega ${install_dir} a tu PATH; por ejemplo:"
       echo "  echo 'export PATH=\"${install_dir}:\$PATH\"' >> ~/.profile && . ~/.profile" ;;
esac
