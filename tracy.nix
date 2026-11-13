{
  lib,
  stdenv,
  fetchFromGitHub,
  coreutils,
  cmake,
  ninja,
  pkg-config,
  wayland-scanner,
  dbus,
  freetype,
  glfw,
  onetbb,
  curl,
  pugixml,
  systemdLibs,
  withGtkFileSelector ? false,
  gtk3,
  withWayland ? stdenv.hostPlatform.isLinux,
  libglvnd,
  libxkbcommon,
  wayland,
  wayland-protocols,
  libffi,
}:

let
  version = "0.14.1";

  cpmSrcs = [
    (fetchFromGitHub {
      name = "capstone";
      owner = "capstone-engine";
      repo = "capstone";
      rev = "6.0.0-Alpha11";
      hash = "sha256-ShiRgCDhzJB3/t/jeiWWa1IQgj254S7/8MGXjD+TUiM="; # Nix will print expected hash if mismatched
    })
    (fetchFromGitHub {
      name = "PPQSort";
      owner = "GabTux";
      repo = "PPQSort";
      rev = "v1.0.6";
      hash = "sha256-HgM+p2QGd9C8A8l/VaEB+cLFDrY2HU6mmXyTNh7xd0A=";
    })
    (fetchFromGitHub {
      name = "PackageProject.cmake";
      owner = "TheLartians";
      repo = "PackageProject.cmake";
      rev = "v1.11.1";
      hash = "sha256-E7WZSYDlss5bidbiWL1uX41Oh6JxBRtfhYsFU19kzIw=";
    })
    (fetchFromGitHub {
      name = "zstd";
      owner = "facebook";
      repo = "zstd";
      rev = "v1.5.7";
      hash = "sha256-tNFWIT9ydfozB8dWcmTMuZLCQmQudTFJIkSr0aG7S44=";
    })
    (fetchFromGitHub {
      name = "json";
      owner = "nlohmann";
      repo = "json";
      rev = "v3.12.0";
      hash = "sha256-cECvDOLxgX7Q9R3IE86Hj9JJUxraDQvhoyPDF03B2CY=";
    })
    (fetchFromGitHub {
      name = "ImGui";
      owner = "ocornut";
      repo = "imgui";
      rev = "v1.92.9b-docking";
      hash = "sha256-PknWLxYuXQ73TCFN+eKOJDNLGbg/ZqKSF6mFxkJG6vI=";
    })
    (fetchFromGitHub {
      name = "nfd";
      owner = "btzy";
      repo = "nativefiledialog-extended";
      rev = "v1.4.0";
      hash = "sha256-WkXDseYMo21Tfz6sMUpSidLuTeXlX+4tPa9VQgupjcw=";
    })
    (fetchFromGitHub {
      name = "md4c";
      owner = "mity";
      repo = "md4c";
      rev = "00788b157b2b6ef3efd8659b66f6521229f382d7";
      hash = "sha256-e19jw4SJXufUgr2qCA+BPiCg16GqoPNGkIQkCqFPeH8=";
    })
    (fetchFromGitHub {
      name = "base64";
      owner = "aklomp";
      repo = "base64";
      rev = "v0.5.2";
      hash = "sha256-dIaNfQ/znpAdg0/vhVNTfoaG7c8eFrdDTI0QDHcghXU=";
    })
    (fetchFromGitHub {
      name = "tidy";
      owner = "htacg";
      repo = "tidy-html5";
      rev = "5.8.0";
      hash = "sha256-vzVWQodwzi3GvC9IcSQniYBsbkJV20iZanF33A0Gpe0=";
    })
    (fetchFromGitHub {
      name = "usearch";
      owner = "unum-cloud";
      repo = "usearch";
      rev = "v2.26.2";
      hash = "sha256-80M4jVxzcubEJm5NYCGhfqTMUftGs0t5L8EenCCuFkk=";
    })
  ];

in
stdenv.mkDerivation {
  pname = "tracy";
  inherit version;

  src = fetchFromGitHub {
    name = "tracy";
    owner = "wolfpld";
    repo = "tracy";
    rev = "v${version}";
    hash = "sha256-vcLI9jb7eYcR162LgBQ2P4A0oiZuYRfYRQiDhlAk5TI=";
  };

  postUnpack = (
    lib.strings.concatLines (
      lib.lists.forEach cpmSrcs (
        s:
        ''
          cp -R ${s.out} ${s.name}
          chmod -R u+w ${s.name}
          appendToVar cmakeFlags -DCPM_${s.name}_SOURCE=$(pwd)/${s.name}
        ''
        + lib.optionalString (s.name == "PPQSort") ''
          mkdir -p PPQSort/cmake
          cp ./tracy/cmake/CPM.cmake PPQSort/cmake/CPM.cmake
        ''
        + lib.optionalString (s.name == "nfd") ''
          mkdir -p nfd/3ps
          rm -rf nfd/3ps/wayland-protocols
          cp -r ${wayland-protocols}/share/wayland-protocols nfd/3ps/wayland-protocols
          chmod -R u+w nfd/3ps/wayland-protocols
        ''
      )
    )
  );

  nativeBuildInputs = [
    cmake
    ninja
    pkg-config
  ]
  ++ lib.optionals stdenv.hostPlatform.isLinux [ wayland-scanner ]
  ++ lib.optionals stdenv.cc.isClang [ stdenv.cc.cc.libllvm ];

  buildInputs = [
    freetype
    onetbb
    curl
    pugixml
  ]
  ++ lib.optionals (stdenv.hostPlatform.isLinux && withGtkFileSelector) [ gtk3 ]
  ++ lib.optionals (stdenv.hostPlatform.isLinux && !withGtkFileSelector) [ dbus systemdLibs ]
  ++ lib.optionals (stdenv.hostPlatform.isLinux && withWayland) [
    libglvnd
    libxkbcommon
    wayland
    wayland-protocols
    libffi
  ]
  ++ lib.optionals (stdenv.hostPlatform.isDarwin || (stdenv.hostPlatform.isLinux && !withWayland)) [
    glfw
  ];

  cmakeFlags = [
    (lib.cmakeBool "DOWNLOAD_FREETYPE" false)
    (lib.cmakeBool "DOWNLOAD_PUGIXML" false)
    (lib.cmakeBool "DOWNLOAD_LIBCURL" false)
    (lib.cmakeBool "CPM_LOCAL_PACKAGES_ONLY" true)
  ]
  ++ lib.optional (stdenv.hostPlatform.isLinux && withGtkFileSelector) (
    lib.cmakeBool "GTK_FILESELECTOR" true
  )
  ++ lib.optional (stdenv.hostPlatform.isLinux && !withWayland) (lib.cmakeBool "LEGACY" true)
  ++ lib.optional (stdenv.hostPlatform.isLinux && withWayland) (
    lib.cmakeFeature "CPM_wayland-protocols_SOURCE" "${wayland-protocols}/share/wayland-protocols"
  );

  env.NIX_CFLAGS_COMPILE = lib.optionalString stdenv.hostPlatform.isLinux "-ltbb";
  dontUseCmakeBuildDir = true;

  configurePhase = ''
    runHook preConfigure

    cmake -B profiler/build -S profiler -G Ninja $cmakeFlags
    appendToVar cmakeFlags "-DPATCH_EXECUTABLE=${coreutils}/bin/true"

    cmake -B capture/build -S capture -G Ninja $cmakeFlags
    cmake -B csvexport/build -S csvexport -G Ninja $cmakeFlags
    cmake -B import/build -S import -G Ninja $cmakeFlags
    cmake -B update/build -S update -G Ninja $cmakeFlags

    runHook postConfigure
  '';

  buildPhase = ''
    runHook preBuild

    ninja -C profiler/build
    ninja -C capture/build
    ninja -C csvexport/build
    ninja -C import/build
    ninja -C update/build

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    install -D -m 0555 profiler/build/tracy-profiler $out/bin/tracy
    install -D -m 0555 profiler/build/tracy-profiler $out/bin/tracy-profiler
    install -D -m 0555 capture/build/tracy-capture $out/bin/tracy-capture
    install -D -m 0555 csvexport/build/tracy-csvexport $out/bin/tracy-csvexport
    install -D -m 0555 import/build/tracy-import-chrome $out/bin/tracy-import-chrome
    install -D -m 0555 import/build/tracy-import-fuchsia $out/bin/tracy-import-fuchsia
    install -D -m 0555 update/build/tracy-update $out/bin/tracy-update

    runHook postInstall
  ''
  + lib.optionalString stdenv.hostPlatform.isLinux ''
    if [ -f extra/desktop/tracy.desktop ]; then
      substituteInPlace extra/desktop/tracy.desktop \
        --replace-quiet Exec=/usr/bin/tracy Exec=tracy || true

      install -D -m 0444 extra/desktop/application-tracy.xml $out/share/mime/packages/application-tracy.xml
      install -D -m 0444 extra/desktop/tracy.desktop $out/share/applications/tracy.desktop
      install -D -m 0444 icon/application-tracy.svg $out/share/icons/hicolor/scalable/apps/application-tracy.svg
      install -D -m 0444 icon/icon.png $out/share/icons/hicolor/256x256/apps/tracy.png
      install -D -m 0444 icon/icon.svg $out/share/icons/hicolor/scalable/apps/tracy.svg
    fi
  '';

  meta = {
    description = "Real time, nanosecond resolution, remote telemetry frame profiler for games and other applications";
    homepage = "https://github.com/wolfpld/tracy";
    license = lib.licenses.bsd3;
    mainProgram = "tracy";
    platforms = lib.platforms.linux ++ lib.optionals (!withWayland) lib.platforms.darwin;
  };
}
