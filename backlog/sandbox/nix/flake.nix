{
  description = "SoulSystem sandbox environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }: {
    devShells.x86_64-linux.default = nixpkgs.legacyPackages.x86_64-linux.mkShell {
      name = "soulsystem-sandbox";
      buildInputs = with nixpkgs.legacyPackages.x86_64-linux; [
        rustc
        cargo
        gcc
        pkg-config
        openssl
      ];
      shellHook = ''
        echo "SoulSystem Nix sandbox ready"
      '';
    };
  };
}
