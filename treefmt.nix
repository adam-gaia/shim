{
  projectRootFile = "flake.nix";
  programs = {
    alejandra.enable = true; # Nix formatter
    rustfmt.enable = true;
    shellcheck.enable = true;
    shfmt.enable = true;
  };
}
