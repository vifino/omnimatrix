{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = {
    self,
    nixpkgs,
    flake-utils,
    rust-overlay,
  }:
    {
      nixosModules.omnimatrix = import ./module.nix;
    }
    // flake-utils.lib.eachSystem ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"]
    (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        checkArgs = {
          inherit self pkgs system;
        };
      in {
        packages = rec {
          omnimatrix = pkgs.callPackage ./. {};
          omnimatrix-static = pkgs.pkgsStatic.callPackage ./. {};
          omnimatrix-coverage = omnimatrix.overrideAttrs (o: {
            RUSTFLAGS = "-C instrument-coverage";
            dontStrip = true;
          });
          default = omnimatrix;
        };
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [pkg-config];
          buildInputs = with pkgs; [
            (rust-bin.stable."1.86.0".default.override {
              extensions = ["llvm-tools-preview"];
            })
            cargo-deny
            cargo-bloat
            cargo-udeps
            cargo-llvm-cov
            llvmPackages_19.bintools
          ];
        };
        formatter = pkgs.alejandra;
      }
    );
}
