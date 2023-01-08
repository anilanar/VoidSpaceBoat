{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-22.11";
  inputs.home.url = "/d8a/Development/Nix/home.nix";

  outputs = { self, nixpkgs, home }:
    home.lib.eachSystem (system:
      let pkgs = import nixpkgs { inherit system; };
      in {
        packages.default = home.packages.${system}.shells.rust {
          extraInputs = with pkgs; [ pkg-config luajit ];
        };
      });
}
