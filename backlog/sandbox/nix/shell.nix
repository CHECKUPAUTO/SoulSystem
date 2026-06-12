{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  name = "soulsystem-sandbox";
  buildInputs = with pkgs; [ rustc cargo gcc pkg-config openssl ];
}
