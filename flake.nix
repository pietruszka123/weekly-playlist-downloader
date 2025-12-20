{
  description = "yes";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
  };

  outputs =
    {
      self,
      nixpkgs,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        system = system;
        config.allowUnfree = true;
      };

      projectName = "bevy-platformer";
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          vscode-langservers-extracted
          rustup
          pkg-config
          cmake
          yt-dlp
        ];
        buildInputs = with pkgs; [
          libevdev
          udev.dev
          pkgs.openssl
          ffmpeg-full
        ];

        shellHook = ''
          printf '\x1b[36m\x1b[1m\x1b[4mTime to develop ${projectName}!\x1b[0m\n\n'
        '';
      };
    };
}
