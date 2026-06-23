#!/usr/bin/env bash
#
# example: FUZZ_DIR=/tmp/fuzz-test PACKAGE=solana-frozen-abi MAX_INPUT_LEN=16 FUZZ_TIME=1s ./ci/fuzz-frozen-abi.sh
#
set -euo pipefail

cd "$(dirname "$0")/.."

export RUST_MIN_STACK="${RUST_MIN_STACK:-16777216}"
export ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=0}"
export LSAN_OPTIONS="${LSAN_OPTIONS:-detect_leaks=0}"

duration="${FUZZ_TIME:-30s}"
fuzz_root="${FUZZ_DIR:-__fuzz__}"
package_filter="${PACKAGE:-}"
manifest_path="${MANIFEST_PATH:-}"
max_input_len="${MAX_INPUT_LEN:-${MAX_LEN:-}}"

bolero_common_args=(
  --features frozen-abi
  --rustc-bootstrap
)

if [[ -z "$manifest_path" && ( -z "$package_filter" || "$package_filter" == "solana-frozen-abi" ) ]]; then
  if [[ -f frozen-abi/Cargo.toml ]]; then
    manifest_path="$PWD/frozen-abi/Cargo.toml"
  else
    patched_frozen_abi_path="$(
      sed -nE 's/^[[:space:]]*solana-frozen-abi[[:space:]]*=[[:space:]]*\{[^}]*path[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' Cargo.toml \
        | tail -n 1
    )"
    if [[ -n "$patched_frozen_abi_path" ]]; then
      if [[ "$patched_frozen_abi_path" != /* ]]; then
        patched_frozen_abi_path="$PWD/$patched_frozen_abi_path"
      fi
      manifest_path="$patched_frozen_abi_path/Cargo.toml"
    fi
  fi
fi

if [[ -d "$manifest_path" ]]; then
  manifest_path="$manifest_path/Cargo.toml"
fi

if [[ -n "$manifest_path" && ! -f "$manifest_path" ]]; then
  echo "manifest path not found: $manifest_path" >&2
  exit 1
fi

bolero_scope_args=()
scope_description="workspace"

if [[ -n "$manifest_path" ]]; then
  bolero_scope_args+=(--manifest-path "$manifest_path")
  scope_description="$manifest_path"

  if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
    export CARGO_TARGET_DIR="$PWD/target/frozen-abi-fuzz"
  fi
fi

if [[ -n "$package_filter" ]]; then
  bolero_scope_args+=(--package "$package_filter")
  scope_description="${scope_description} package $package_filter"
fi

if ((${#bolero_scope_args[@]} == 0)); then
  bolero_scope_args+=(--workspace)
fi

bolero_list_args=(
  "${bolero_common_args[@]}"
  "${bolero_scope_args[@]}"
)

packages=()
targets=()
echo "listing frozen ABI fuzz targets from $scope_description" >&2
target_list="$(cargo bolero list "${bolero_list_args[@]}")"

# prepare list of fuzz enabled targets
while IFS= read -r line; do
  package=""
  target=""

  if [[ "$line" =~ \"package\":\"([^\"]+)\" ]]; then
    package="${BASH_REMATCH[1]}"
  fi

  if [[ "$line" =~ \"test\":\"([^\"]*_frozen_abi_fuzzer::test_fuzzer_[^\"]+)\" ]]; then
    target="${BASH_REMATCH[1]}"
  fi

  if [[ -n "$target" ]]; then
    if [[ -z "$package" ]]; then
      package="$package_filter"
    fi
    packages+=("$package")
    targets+=("$target")
  fi
done <<< "$target_list"

if ((${#targets[@]} == 0)); then
  echo "no fuzzer targets found for $scope_description" >&2
  exit 1
fi

echo "found ${#targets[@]} frozen ABI fuzz target(s)" >&2

for i in "${!targets[@]}"; do
  package="${packages[$i]}"
  target="${targets[$i]}"
  target_dir="${target//::/__}"
  package_dir="${package:-workspace}"
  work_dir="$fuzz_root/$package_dir/$target_dir"

  bolero_test_args=(
    "${bolero_common_args[@]}"
    "${bolero_scope_args[@]}"
    --corpus-dir "$work_dir/corpus"
    --crashes-dir "$work_dir/crashes"
  )

  if [[ -n "$max_input_len" ]]; then
    bolero_test_args+=(--max-input-length "$max_input_len")
  fi

  echo "fuzzing ${package:+$package::}$target for $duration${max_input_len:+ max_input_len=$max_input_len}"

  cargo bolero test "$target" "${bolero_test_args[@]}" -T "$duration"
done
