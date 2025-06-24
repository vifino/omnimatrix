{
  lib,
  rustPlatform,
  pkg-config,
  openssl,
  ...
}:
rustPlatform.buildRustPackage {
  pname = "omnimatrix";
  version = "0.1.0";
  src = with lib.strings;
    builtins.filterSource
    (path: type: builtins.any (suf: hasPrefix (toString suf) path) [./src ./Cargo.toml ./Cargo.lock ./crates])
    ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  meta = with lib; {
    mainProgram = "omnimatrix";
    platforms = platforms.unix;
    license = licenses.isc;
    maintainers = [maintainers.vifino];
  };
}
