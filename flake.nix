{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    systems.url = "github:nix-systems/default";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs = inputs: inputs.flake-parts.lib.mkFlake { inherit inputs; } {
    systems = import inputs.systems;
    perSystem = { pkgs, ... }: {
      packages.default = pkgs.callPackage ./. { };
      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          rustc
          cargo
          clippy
        ];
      };
    };
  };
}
