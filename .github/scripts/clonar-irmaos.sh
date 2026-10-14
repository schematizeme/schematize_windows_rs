#!/usr/bin/env bash
# TEMP(ADR-0026): enquanto as dependências são `path = "../…"`, o CI clona os repos irmãos (nas
# tags de publicação) ao lado deste checkout. Ao publicar (F7), as deps viram `git + tag`, este
# script e os passos que o chamam são APAGADOS.
#
# Layout esperado: <WORKSPACE>/schematize_windows_rs (este repo) e <WORKSPACE>/<irmão>.
# Repo privado: exporte CI_READ_TOKEN (PAT com leitura nos repos irmãos).
set -euo pipefail
export GIT_TERMINAL_PROMPT=0
raiz="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$raiz/.."

clonar() {
  local repo="$1" tag="$2"
  if [ -d "$repo" ]; then
    echo "ok: $repo já existe"
    return 0
  fi
  local url="https://github.com/schematizeme/${repo}.git"
  if [ -n "${CI_READ_TOKEN:-}" ]; then
    url="https://x-access-token:${CI_READ_TOKEN}@github.com/schematizeme/${repo}.git"
  fi
  git clone --quiet --depth 1 --branch "$tag" "$url" "$repo"
  echo "ok: $repo @ $tag"
}

clonar schematize_plataforma_rs v0.2.2
clonar schematize_market_rs v0.6.0
clonar schematize_overdev_rs v0.4.0
