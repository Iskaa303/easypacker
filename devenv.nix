{ pkgs, ... }: {
  packages = with pkgs; [ cargo rustfmt clippy ];
}
