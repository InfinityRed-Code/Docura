<#
  Build the corresponding-source archive for the Tesseract bundle that the
  Windows installer ships (scripts/fetch-tesseract.ps1).

  Attach the resulting archive to every GitHub release whose installer
  contains this Tesseract bundle; THIRD-PARTY-NOTICES.txt in the installed
  tesseract folder points there. It is NOT part of the installer.

  What is inside, everything pinned by SHA-256:
    tesseract/  Tesseract source at the UB Mannheim tag the binaries were built
                from (v5.4.0.20240606, commit 38bfd186). Contains the build
                scripts UB Mannheim used (.github/workflows/build.sh,
                make-installer.sh, nsis/).
    msys2/      For every bundled MSYS2 library DLL: the MSYS2 build recipe
                (PKGBUILD and patches from msys2/MINGW-packages at the commit
                of the package release the binaries came from) plus the
                upstream source tarball it builds, i.e. the contents of the
                MSYS2 source package <name>-<pkgver>-<pkgrel>.src.tar.zst.
                MSYS2 no longer hosts most of these old source packages, so
                they are rebuilt from the same inputs here.

  How the package releases were determined: UB Mannheim's build installs the
  MSYS2 packages current at build time (2024-06-06). Each release below is
  the MINGW-packages state before that date, matches the version string in
  the DLL, and for every package still downloadable from repo.msys2.org
  (jbigkit, bzip2, giflib, lerc, libb2) the bundled DLL is byte-identical to
  the one in that package.

  Not included: the GCC runtime (libgcc_s_seh-1.dll, libstdc++-6.dll; GCC
  Runtime Library Exception) and libwinpthread-1.dll (MIT/BSD); README.txt
  in the archive says where their source is.

  Usage:  powershell -ExecutionPolicy Bypass -File scripts\fetch-tesseract-sources.ps1 [-OutDir dist] [-Force]
          (or: just tesseract-sources)
#>
param(
    [string] $OutDir,
    [switch] $Force
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$Bundle = '5.4.0.20240606'   # must match $Version in fetch-tesseract.ps1
$Name   = "docura-pdf-tesseract-sources-$Bundle"

$Tesseract = @{
    File   = "tesseract-$Bundle.tar.gz"
    Url    = "https://github.com/UB-Mannheim/tesseract/archive/refs/tags/v$Bundle.tar.gz"
    Sha256 = 'd2710c75903ae24ef5325069f4f2ada7c0760326b39bca2b9864124ae0992e7b'
}

# Dir = msys2/MINGW-packages directory without the "mingw-w64-" prefix.
# Recipe = files of that directory at Commit; Upstream = remote sources from
# the PKGBUILD's source=() (hashes as in its sha256sums=(); signatures,
# which MSYS2 marks SKIP, are left out).
$Packages = @(
    @{ Dir = 'bzip2'; Release = '1.0.8-3'; Dlls = 'libbz2-1.dll'
       Commit = '619a8198e55722d2b0fcd95292d3ccdc98148665'
       Recipe = [ordered]@{
           'PKGBUILD' = '40131efa9802a81168c97d685fdf98c8e24e8f3c6e12477d7a235ef651f60820'
           'bzip2-1.0.6-progress.all.patch' = 'f93e6b50082a8e880ee8436c7ec6a65a8f01e9282436af77f95bb259b1c7f7f7'
           'bzip2-buildsystem.all.patch' = 'e519a50a4adaf05b18650d3e9d644badc66e80ca86063681ffe9a2c2226319d0'
           'bzip2-cygming-1.0.6.src.all.patch' = '7e67f77172b19f3e6c1f0875b1d3e9cb79211f8e1c752794ef9afd3704f928cf' }
       Upstream = @(@{ File = 'bzip2-1.0.8.tar.gz'; Url = 'https://sourceware.org/pub/bzip2/bzip2-1.0.8.tar.gz'
                       Sha256 = 'ab5a03176ee106d3f0fa90e381da478ddae405918153cca248e682cd0c4a2269' }) }
    @{ Dir = 'expat'; Release = '2.6.2-1'; Dlls = 'libexpat-1.dll'
       Commit = '619a8198e55722d2b0fcd95292d3ccdc98148665'
       Recipe = [ordered]@{ 'PKGBUILD' = '6c85c1be0d343eb9dc38d5abbaf955ee3663083ccf1567f525b3db9fb8ec90f3' }
       Upstream = @(@{ File = 'expat-2.6.2.tar.xz'; Url = 'https://github.com/libexpat/libexpat/releases/download/R_2_6_2/expat-2.6.2.tar.xz'
                       Sha256 = 'ee14b4c5d8908b1bec37ad937607eab183d4d9806a08adee472c3c3121d27364' }) }
    @{ Dir = 'giflib'; Release = '5.2.2-1'; Dlls = 'libgif-7.dll'
       Commit = '619a8198e55722d2b0fcd95292d3ccdc98148665'
       Recipe = [ordered]@{
           'PKGBUILD' = '23602d6ea7fe9d0a1e1ebab89c70d08a27511f95e4e6ba731f7946090f4f5586'
           '001-mingw-build.patch' = '57f4da1dd467a13c92b07347d00f4675738ea5ba32d93195b8b7f1eb2cf8bc9d'
           'giflib_quantize.patch' = '29f0d5a98520cf908a11325987ca9ef2454ca8cb85ce9815b75a52956b9df28a' }
       Upstream = @(@{ File = 'giflib-5.2.2.tar.gz'; Url = 'https://downloads.sourceforge.net/sourceforge/giflib/giflib-5.2.2.tar.gz'
                       Sha256 = 'be7ffbd057cadebe2aa144542fd90c6838c6a083b5e8a9048b8ee3b66b29d5fb' }) }
    @{ Dir = 'jbigkit'; Release = '2.1-5'; Dlls = 'libjbig-0.dll'
       Commit = '828182ad91f260eba55f388ef5f9d310e888711c'
       Recipe = [ordered]@{
           'PKGBUILD' = '95e0df2be8fb755c7e9a0ccbba56412fed191d34e13c6a9d55c3469c029425a8'
           'allNewMainMakefile.all.patch' = 'ce210309c47214599c451e680043dc827980ace647385c4ae26f3725d1054d97'
           'manfiles.all.patch' = 'd5cce4c1bae575563cb892c93de349a7747435c175e81ee57d77e65079d11908'
           'pbmtoolsMakefile.all.patch' = '7f268260d596275a1bbe803a829baf6c4a6938f9e23e861dd433622e12c2487e'
           'useLibtool.all.patch' = '10877f4c14c1251871b8d733b9748d8db66cb4d7e0453cfd92a5b30e1c268e5e'
           'zzz_debug.mingw.patch' = 'fe579b90d3b34e6d3209af91fcd9f84a172c2e45ca88a8b23e84f8eacd04f941' }
       Upstream = @(@{ File = 'jbigkit-2.1.tar.gz'; Url = 'https://www.cl.cam.ac.uk/~mgk25/download/jbigkit-2.1.tar.gz'
                       Sha256 = 'de7106b6bfaf495d6865c7dd7ac6ca1381bd12e0d81405ea81e7f2167263d932' }) }
    @{ Dir = 'leptonica'; Release = '1.84.1-1'; Dlls = 'libleptonica-6.dll'
       Commit = '619a8198e55722d2b0fcd95292d3ccdc98148665'
       Recipe = [ordered]@{ 'PKGBUILD' = '72ab6e88561f00ff7f06367a66b3f63fb68cd97d183535d550b4331dd7107b3e' }
       Upstream = @(@{ File = 'leptonica-1.84.1.tar.gz'; Url = 'https://github.com/DanBloomberg/leptonica/releases/download/1.84.1/leptonica-1.84.1.tar.gz'
                       Sha256 = '2b3e1254b1cca381e77c819b59ca99774ff43530209b9aeb511e1d46588a64f6' }) }
    @{ Dir = 'lerc'; Release = '4.0.0-1'; Dlls = 'libLerc.dll'
       Commit = '5f5f5e29136b5614d9dcb8ae7f70a69828708644'
       Recipe = [ordered]@{ 'PKGBUILD' = 'bacc823d6a4fb7d22f60c4da38f465be630b2d6cdc7e780477d45bb42e8efd6a' }
       Upstream = @(@{ File = 'lerc-4.0.0.tar.gz'; Url = 'https://github.com/Esri/lerc/archive/v4.0.0.tar.gz'
                       Sha256 = '91431c2b16d0e3de6cbaea188603359f87caed08259a645fd5a3805784ee30a0' }) }
    @{ Dir = 'libarchive'; Release = '3.7.4-1'; Dlls = 'libarchive-13.dll'
       Commit = '34cd7b3b9be151ee8c22f70cf1c8d73e5389803f'
       Recipe = [ordered]@{ 'PKGBUILD' = 'a321969a68a1ff478a83f27248c2a5e2483a9db8f8b4a4fe0a799a0fc98bf713' }
       Upstream = @(@{ File = 'libarchive-3.7.4.tar.xz'; Url = 'https://github.com/libarchive/libarchive/releases/download/v3.7.4/libarchive-3.7.4.tar.xz'
                       Sha256 = 'f887755c434a736a609cbd28d87ddbfbe9d6a3bb5b703c22c02f6af80a802735' }) }
    @{ Dir = 'libb2'; Release = '0.98.1-2'; Dlls = 'libb2-1.dll'
       Commit = '98711b457c7f1b692d8b2c6d5d761f5cb5a1cfba'
       Recipe = [ordered]@{ 'PKGBUILD' = 'a0a4fd283698b56f0eb867fa63aa350e8ee5224c9e64eff076efc4f13617b696' }
       Upstream = @(@{ File = 'libb2-0.98.1.tar.gz'; Url = 'https://github.com/BLAKE2/libb2/releases/download/v0.98.1/libb2-0.98.1.tar.gz'
                       Sha256 = '53626fddce753c454a3fea581cbbc7fe9bbcf0bc70416d48fdbbf5d87ef6c72e' }) }
    @{ Dir = 'libdeflate'; Release = '1.20-1'; Dlls = 'libdeflate.dll'
       Commit = 'd93bb53e0e0456be10ceb4710c3f337e74143db9'
       Recipe = [ordered]@{
           'PKGBUILD' = '0e8390a7c0672169d0b585fb9d51826bc8976f99a4b600984fb3e0f68adc4f80'
           '002-pkg-config.patch' = '7feb96b6466375824af54a829dfe0d5cbd641970d64ef9fe0f9f4ab5e18040dd' }
       Upstream = @(@{ File = 'libdeflate-1.20.tar.gz'; Url = 'https://github.com/ebiggers/libdeflate/archive/v1.20.tar.gz'
                       Sha256 = 'ed1454166ced78913ff3809870a4005b7170a6fd30767dc478a09b96847b9c2a' }) }
    @{ Dir = 'libiconv'; Release = '1.17-4'; Dlls = 'libiconv-2.dll'
       Commit = '093ab192533820af5b5d8a333db35e550ec89829'
       Recipe = [ordered]@{
           'PKGBUILD' = '5a6abee1e5a1fbf1f80bca19625bac990c94e0aee92cf03a856e86e1b486facf'
           '0002-fix-cr-for-awk-in-configure.all.patch' = '89d5c0f666e50a0186cfb142ba7b77e8fe1ac4d65bdfd9ae14ae8d2f0045a87c'
           '0003-add-cp65001-as-utf8-alias.patch' = 'cb2b1cca44c9b9bd11c3bd33997c10d22dff263542275324c88f34b59910ef87'
           '0004-fix-makefile-devel-assuming-gcc.patch' = '2f08ca8681e4c85cdb2e6835921a502fd623dee4bf8ca10aa3c66747ede58a47'
           'fix-pointer-buf.patch' = '6b6e2393840f4dc6067587165777b1a07978f4c05247d7c1010a45ad251bbeeb'
           'iconv.pc' = '56e7ec406bf42eb66b1d972f20d229f7a76ba00c38c7b4403bf348875704baae' }
       Upstream = @(@{ File = 'libiconv-1.17.tar.gz'; Url = 'https://ftp.gnu.org/pub/gnu/libiconv/libiconv-1.17.tar.gz'
                       Sha256 = '8f74213b56238c85a50a5329f77e06198771e70dd9a739779f4c02f65d971313' }) }
    @{ Dir = 'libjpeg-turbo'; Release = '3.0.3-1'; Dlls = 'libjpeg-8.dll'
       Commit = 'cb7602973347f82e8e9167644a3ff69629c0fdea'
       Recipe = [ordered]@{
           'PKGBUILD' = 'cfc751a1ec0e8e31def405e23f40ae4146aee520fb1353eb3ba7d1287291df70'
           '0001-header-compat.mingw.patch' = '166264f617734b33ac55ec8b70e6636d4e409c0d837667afe158e9d28200e6dd'
           'libjpeg-turbo-1.3.1-libmng-compatibility.patch' = '16336caddc949a8a082f39c218b3289288d144bb3b87f62565ed1b294ff8e526' }
       Upstream = @(@{ File = 'libjpeg-turbo-3.0.3.tar.gz'; Url = 'https://github.com/libjpeg-turbo/libjpeg-turbo/releases/download/3.0.3/libjpeg-turbo-3.0.3.tar.gz'
                       Sha256 = '343e789069fc7afbcdfe44dbba7dbbf45afa98a15150e079a38e60e44578865d' }) }
    @{ Dir = 'libpng'; Release = '1.6.43-1'; Dlls = 'libpng16-16.dll'
       Commit = '619a8198e55722d2b0fcd95292d3ccdc98148665'
       Recipe = [ordered]@{ 'PKGBUILD' = 'e664ec7f319f9cf444da276a24e1e4d1ede60baeccb48e966772731d1bcfe3a7' }
       Upstream = @(
           @{ File = 'libpng-1.6.43.tar.xz'; Url = 'https://downloads.sourceforge.net/sourceforge/libpng/libpng-1.6.43.tar.xz'
              Sha256 = '6a5ca0652392a2d7c9db2ae5b40210843c0bbc081cbd410825ab00cc59f14a6c' }
           @{ File = 'apng.patch'; Url = 'https://raw.githubusercontent.com/mozilla/gecko-dev/ee4f298265cc851314bf431fb0efccba6af15f74/media/libpng/apng.patch'
              Sha256 = '2a65fa19b126e85ad14ef1d1d9fc5079193e92af6b8db4454e3a875da5fdb79d' }) }
    @{ Dir = 'libtiff'; Release = '4.6.0-1'; Dlls = 'libtiff-6.dll'
       Commit = '619a8198e55722d2b0fcd95292d3ccdc98148665'
       Recipe = [ordered]@{
           'PKGBUILD' = '1db1c7089a7ae2512412d6c954d9cf72b1f832ddf3a7d4627479e44ea6b44124'
           '0002-libtiff-install-headers.patch' = '493742947c8667655b6b89f2d7d27e92e1438a490ed86f50811112394b432a12' }
       Upstream = @(@{ File = 'tiff-4.6.0.tar.gz'; Url = 'https://download.osgeo.org/libtiff/tiff-4.6.0.tar.gz'
                       Sha256 = '88b3979e6d5c7e32b50d7ec72fb15af724f6ab2cbf7e10880c360a77e4b5d99a' }) }
    @{ Dir = 'libwebp'; Release = '1.4.0-1'; Dlls = 'libwebp-7.dll, libwebpmux-3.dll, libsharpyuv-0.dll'
       Commit = '3e65265c55b033533fcebf0a34ac781cc4184c61'
       Recipe = [ordered]@{
           'PKGBUILD' = '4e2626db6ac745b3ab5b50ba0c2c7e8fd5f2d357d6a1b49aeca979400509b810'
           '0001-mingw-cmake-output.patch' = '30ed42b782af427f57049abada30ce6d4d62318ee13014585cb352b570c68e3d' }
       Upstream = @(@{ File = 'libwebp-1.4.0.tar.gz'; Url = 'https://github.com/webmproject/libwebp/archive/v1.4.0.tar.gz'
                       Sha256 = '12af50c45530f0a292d39a88d952637e43fb2d4ab1883c44ae729840f7273381' }) }
    @{ Dir = 'lz4'; Release = '1.9.4-1'; Dlls = 'liblz4.dll'
       Commit = '619a8198e55722d2b0fcd95292d3ccdc98148665'
       Recipe = [ordered]@{ 'PKGBUILD' = '4b6e9ca1e93bcdd251e8b8a472c7a0b7af3b440e22e1617ec1cb6564ceb73fca' }
       Upstream = @(@{ File = 'lz4-1.9.4.tar.gz'; Url = 'https://github.com/lz4/lz4/archive/v1.9.4.tar.gz'
                       Sha256 = '0b0e3aa07c8c063ddf40b082bdf7e37a1562bda40a0ff5272957f3e987e0e54b' }) }
    @{ Dir = 'openjpeg2'; Release = '2.5.2-1'; Dlls = 'libopenjp2-7.dll'
       Commit = '619a8198e55722d2b0fcd95292d3ccdc98148665'
       Recipe = [ordered]@{
           'PKGBUILD' = 'f97cdc8248adcc5075218fc098d0df7e5b967865207e102eb9396168f4f02746'
           '0004-mingw-disable-java.patch' = 'adade9b29e4292b642d547644e26fab6e193bbfe31d25f07d89a8c35e4d0993d'
           '0006-openjpeg-2.1.0-stdcall-for-all-win.patch' = '44679801b740ddb75edd4b55031c5f70a02eaf4f8e5eba5f3ad1b397b2ec4aed' }
       Upstream = @(@{ File = 'openjpeg-2.5.2.tar.gz'; Url = 'https://github.com/uclouvain/openjpeg/archive/v2.5.2.tar.gz'
                       Sha256 = '90e3896fed910c376aaf79cdd98bdfdaf98c6472efd8e1debf0a854938cbda6a' }) }
    @{ Dir = 'openssl'; Release = '3.3.1-1'; Dlls = 'libcrypto-3-x64.dll'
       Commit = '11b59a9a9ee551e8ff635c5b942625b22c0355a2'
       Recipe = [ordered]@{
           'PKGBUILD' = '0e3e01d18f9e3c2ae3f9ad8d194a364cb5dd0d2211f6d69acc66029ba253b639'
           '001-support-aarch64.patch' = '21b96771b401442570e885c2d5689a359a91e86dcbf5511db3667202b6c1fa8a'
           '002-relocation.patch' = '8a2d91826bd1a8fd6d3a981d2a71cfb3170060f64a053eafd8e3b32f6a30ac3b'
           'pathtools.c' = 'ebf471173f5ee9c4416c10a78760cea8afaf1a4a6e653977321e8547ce7bf3c0'
           'pathtools.h' = '1585ef1b61cf53a2ca27049c11d49e0834683dfda798f03547761375df482a90' }
       Upstream = @(@{ File = 'openssl-3.3.1.tar.gz'; Url = 'https://github.com/openssl/openssl/releases/download/openssl-3.3.1/openssl-3.3.1.tar.gz'
                       Sha256 = '777cd596284c883375a2a7a11bf5d2786fc5413255efab20c50d6ffe6d020b7e' }) }
    @{ Dir = 'xz'; Release = '5.6.2-2'; Dlls = 'liblzma-5.dll'
       Commit = '9011bda67607b97a8d126d2331572e69f583f861'
       Recipe = [ordered]@{ 'PKGBUILD' = '32a3128d04b593dd52748e72f36710baba688ebe228abbd872fe77e7c1ae272d' }
       Upstream = @(@{ File = 'xz-5.6.2.tar.xz'; Url = 'https://github.com/tukaani-project/xz/releases/download/v5.6.2/xz-5.6.2.tar.xz'
                       Sha256 = 'a9db3bb3d64e248a0fae963f8fb6ba851a26ba1822e504dc0efd18a80c626caf' }) }
    @{ Dir = 'zlib'; Release = '1.3.1-1'; Dlls = 'zlib1.dll'
       Commit = '619a8198e55722d2b0fcd95292d3ccdc98148665'
       Recipe = [ordered]@{
           'PKGBUILD' = 'dada944be7fdbc675a02f7b8ede171e310221489288345257050c3e2091429d1'
           '01-zlib-1.2.11-1-buildsys.mingw.patch' = '8175e70c980ffc48a2e90c6f0431ddb80ef6ff690f9e8906dd6a756238552fa5'
           '03-dont-put-sodir-into-L.mingw.patch' = '7287d12db57dcf0f66964c861100bf06ebe1ddcb243e7ee0773fcab1b2935596'
           '04-fix-largefile-support.patch' = '4d8ebda55b95d19cd7c467c486d4c42d2bad6db9ff5dca361f6d8c24120e5f30'
           '07-Add-no-undefined-to-link-to-enable-build-shared-vers.patch' = 'fb83292f494f649ea7f1835aa4abea61acc593d7f90625741d42cd99cac0e075'
           '08-Add-bzip2-library-to-pkg-config-file.patch' = '0be98a7e660e7c068856f08b9e92d6c73a6b0d88c3e29a9716140b147f7c23c5' }
       Upstream = @(@{ File = 'zlib-1.3.1.tar.xz'; Url = 'https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.xz'
                       Sha256 = '38ef96b8dfe510d42707d9c781877914792541133e1870841463bfa73f883e32' }) }
    @{ Dir = 'zstd'; Release = '1.5.6-2'; Dlls = 'libzstd.dll'
       Commit = 'f40f141bf39985484f81a03bebe511ebc29733a4'
       Recipe = [ordered]@{
           'PKGBUILD' = '7670b7adc88057d72a34760d550503a4c0ed4dd394d45a131fba69c618d73403'
           'zstd-1.4.0-fileio-mingw.patch' = '95adbf87b0c286df98ed179d35f0df70accf2e70740885146dae8eec57331713' }
       Upstream = @(@{ File = 'zstd-1.5.6.tar.gz'; Url = 'https://github.com/facebook/zstd/releases/download/v1.5.6/zstd-1.5.6.tar.gz'
                       Sha256 = '8c29e06cf42aacc1eafc4077ae2ec6c6fcb96a626157e0593d5e82a34fd403c1' }) }
)

$Root = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $Root 'dist' }
$Zip = Join-Path $OutDir "$Name.zip"
if ((Test-Path $Zip) -and -not $Force) {
    Write-Host "exists: $Zip (use -Force to rebuild)"
    exit 0
}

function Get-Verified([string] $Url, [string] $OutFile, [string] $Sha256) {
    Write-Host "download: $Url"
    for ($try = 1; ; $try++) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $OutFile -UserAgent 'Wget/1.21'
            break
        } catch {
            if ($try -ge 3) { throw "download failed: $Url`n$($_.Exception.Message)" }
            Start-Sleep -Seconds (5 * $try)
        }
    }
    $actual = (Get-FileHash -Algorithm SHA256 $OutFile).Hash
    if ($actual -ne $Sha256.ToUpperInvariant()) {
        throw "SHA-256 mismatch for $Url`n  expected $Sha256`n  actual   $actual"
    }
}

$work = Join-Path ([IO.Path]::GetTempPath()) ("docura-tesseract-src-" + [guid]::NewGuid().ToString('N'))
$top = Join-Path $work $Name
try {
    New-Item -ItemType Directory -Force -Path (Join-Path $top 'tesseract') | Out-Null
    Get-Verified $Tesseract.Url (Join-Path $top "tesseract\$($Tesseract.File)") $Tesseract.Sha256

    $table = New-Object System.Collections.Generic.List[string]
    foreach ($p in $Packages) {
        $pkgDir = Join-Path $top "msys2\mingw-w64-$($p.Dir)-$($p.Release)"
        New-Item -ItemType Directory -Force -Path $pkgDir | Out-Null
        foreach ($file in $p.Recipe.Keys) {
            $url = "https://raw.githubusercontent.com/msys2/MINGW-packages/$($p.Commit)/mingw-w64-$($p.Dir)/$file"
            Get-Verified $url (Join-Path $pkgDir $file) $p.Recipe[$file]
        }
        foreach ($u in $p.Upstream) {
            Get-Verified $u.Url (Join-Path $pkgDir $u.File) $u.Sha256
        }
        $table.Add(('  {0,-34} {1}  {2}' -f "mingw-w64-$($p.Dir) $($p.Release)", $p.Commit.Substring(0, 12), $p.Dlls))
    }

    $readme = @"
Corresponding source for the Tesseract OCR bundle of Docura PDF
================================================================

Bundle: Tesseract $Bundle (UB Mannheim Windows build), installed by the
Docura PDF Windows installer into <install dir>\tesseract. Docura PDF runs
tesseract.exe as a separate program; this folder is under the licenses of
its components (see tesseract\THIRD-PARTY-NOTICES.txt and licenses\ there),
not under the Docura PDF license.

tesseract\tesseract-$Bundle.tar.gz
  Tesseract source at tag v$Bundle of https://github.com/UB-Mannheim/tesseract
  (commit 38bfd186cc92e483f24d6257cc4b5e86c4b406b2), from which
  tesseract.exe and libtesseract-5.dll were built. UB Mannheim's build
  scripts are inside: .github/workflows/build.sh, make-installer.sh, nsis/.
  The build cross-compiles with MinGW-w64 against the MSYS2 packages below.

msys2\mingw-w64-<name>-<pkgver>-<pkgrel>\
  Contents of the MSYS2 source package of the same name: the build recipe
  (PKGBUILD and patches, from https://github.com/msys2/MINGW-packages at the
  commit listed below) and the upstream source tarball(s) it builds.
  Build with MSYS2's makepkg-mingw in that directory.

  MSYS2 package release              Commit        Bundled DLL(s)
$($table -join "`r`n")

  These are the releases current in MSYS2 when the bundle was built
  (2024-06-06). For jbigkit, bzip2, giflib, lerc and libb2, whose binary
  packages are still on repo.msys2.org, the bundled DLLs were checked to be
  byte-identical to the packaged ones.

Not included, with where to get the source:
  libgcc_s_seh-1.dll, libstdc++-6.dll
    GCC 14.1.0 runtime (MSYS2 mingw-w64-gcc-libs 14.1.0-3), GPL-3.0-or-later
    WITH GCC-exception-3.1. Source: https://ftp.gnu.org/gnu/gcc/gcc-14.1.0/
    and https://github.com/msys2/MINGW-packages/tree/3127522154b79ff94dedc07a5c5a613c98c17a41/mingw-w64-gcc
  libwinpthread-1.dll
    mingw-w64 winpthreads (MSYS2 mingw-w64-libwinpthread-git
    12.0.0.r32.gf977e1c38-1), MIT and BSD-3-Clause. Source:
    https://github.com/mingw-w64/mingw-w64/tree/f977e1c38/mingw-w64-libraries/winpthreads
  tessdata\*.traineddata are data files (tessdata_fast 4.1.0,
    https://github.com/tesseract-ocr/tessdata_fast), Apache-2.0.

SHA256SUMS.txt lists every file in this archive.
"@
    Set-Content -Path (Join-Path $top 'README.txt') -Value $readme -Encoding ASCII

    $sums = Get-ChildItem $top -Recurse -File | Sort-Object FullName | ForEach-Object {
        '{0}  {1}' -f (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLowerInvariant(),
            $_.FullName.Substring($top.Length + 1).Replace('\', '/')
    }
    # LF line ends, so `sha256sum -c SHA256SUMS.txt` works on Linux too.
    [IO.File]::WriteAllText((Join-Path $top 'SHA256SUMS.txt'), (($sums -join "`n") + "`n"), [Text.Encoding]::ASCII)

    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
    if (Test-Path $Zip) { Remove-Item -Force $Zip }
    # bsdtar from Windows 10+: -a picks the zip format from the extension.
    & (Join-Path $env:SystemRoot 'System32\tar.exe') -a -c -f $Zip -C $work $Name
    if ($LASTEXITCODE -ne 0) { throw "tar failed (exit $LASTEXITCODE)" }
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

$sizeMb = [math]::Round((Get-Item $Zip).Length / 1MB, 1)
$hash = (Get-FileHash -Algorithm SHA256 $Zip).Hash.ToLowerInvariant()
Write-Host "source archive: $Zip ($sizeMb MB)"
Write-Host "sha256: $hash"
Write-Host "Attach it to the GitHub release, e.g.: gh release upload v<version> `"$Zip`""
