{
  lib,
  rustPlatform,

  coreutils,
  pkg-config,
  openssl,
  perl,
}:
rustPlatform.buildRustPackage {
  name = "omni";
  src = lib.cleanSource ./.;

  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    perl
  ];

  buildInputs = [
    openssl
  ];

  propagatedBuildInputs = [
    coreutils
  ];

  checkPhase = ''
    cargo test -- \
      --skip internal::cache::up_environments::tests::up_environment::test_new_and_init \
      --skip internal::config::up::cargo_install::tests::install \
      --skip internal::config::up::github_release::tests::up
  '';
}
