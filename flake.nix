{
  description = "git-shim";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nonstdlib.url = "github:shell-lib/nonstdlib";
    git-track-repos.url = "gitlab:adam_gaia/git-track-repos";
  };
  outputs = { self, nixpkgs, flake-utils, nonstdlib, git-track-repos, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        package-name = "git-shim";
        runtime-dependencies = [ 
          nonstdlib.packages.${system}.default
          git-track-repos.packages.${system}.default
        ];
      in rec {
        devShell = pkgs.mkShell {
          buildInputs = [
            pkgs.shellcheck
            pkgs.shfmt
          ] ++ runtime-dependencies;
        };

        defaultPackage = pkgs.writeShellApplication {
          name = package-name;
          runtimeInputs = runtime-dependencies;
          text = (builtins.readFile ./${package-name});
        };
      });
}
