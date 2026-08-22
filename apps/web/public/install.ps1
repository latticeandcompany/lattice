# Lattice installer for Windows. Run from the root of the repo you want to use it in:
#
#   irm https://latticeandcompany.github.io/lattice/install.ps1 | iex
#
# Everything lands in .\.lattice\bin — no global paths, no package manager. That
# directory is then added to your user PATH, so `lattice` in this repo means the
# version this repo pins. To skip that edit, pass the switch through a scriptblock
# (`irm | iex` cannot forward arguments):
#
#   & ([scriptblock]::Create((irm https://latticeandcompany.github.io/lattice/install.ps1))) -NoModifyPath
#
# or set $env:LATTICE_NO_PATH = '1'. To remove it: rd /s .lattice, and drop the
# entry from your user PATH.
#
# Which version it installs, in order:
#   1. $env:LATTICE_VERSION, if set
#   2. latticeVersion from .\lattice.json — the committed config is the lockfile,
#      which is why this is read here rather than by a binary that does not exist yet
#   3. the newest release, when the directory has no lattice.json at all
#
# A lattice.json that exists but pins nothing is an error, not a reason to guess.
#
# In Git Bash, MSYS2, Cygwin or WSL2, use install.sh instead.

[CmdletBinding()]
param(
	[switch] $NoModifyPath,
	[switch] $AssumeYes
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Invoke-WebRequest renders a progress bar per chunk, which costs more than the
# download on Windows PowerShell.
$ProgressPreference = 'SilentlyContinue'

# Windows PowerShell 5.1 still negotiates SSL3/TLS1.0 by default; github.com
# serves neither.
try {
	[Net.ServicePointManager]::SecurityProtocol =
		[Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch {}

$Repo = 'latticeandcompany/lattice'
$BaseUrl = if ($env:LATTICE_RELEASE_BASE_URL) { $env:LATTICE_RELEASE_BASE_URL } else { "https://github.com/$Repo/releases/download" }
$LatestUrl = if ($env:LATTICE_RELEASE_LATEST_URL) { $env:LATTICE_RELEASE_LATEST_URL } else { "https://api.github.com/repos/$Repo/releases/latest" }
$ListUrl = if ($env:LATTICE_RELEASE_LIST_URL) { $env:LATTICE_RELEASE_LIST_URL } else { "https://api.github.com/repos/$Repo/releases?per_page=20" }

function Say { param([string] $Text, [ConsoleColor] $Color = [ConsoleColor]::Gray)
	Write-Host $Text -ForegroundColor $Color
}
function Dim { param([string] $Text) Say $Text ([ConsoleColor]::DarkGray) }
function Die { param([string] $Text)
	Write-Host 'error: ' -ForegroundColor Red -NoNewline
	Write-Host $Text
	# Not `exit`: `irm | iex` runs this in the caller's scope, where exiting closes
	# their console. A terminating error stops the script either way.
	throw 'lattice was not installed'
}

if ($env:LATTICE_NO_PATH) { $NoModifyPath = $true }
if ($env:LATTICE_ASSUME_YES) { $AssumeYes = $true }

# --- prerequisites -----------------------------------------------------------
# bsdtar has shipped in Windows since 10 1803, and the release archives are
# .tar.gz for every platform, which Expand-Archive cannot read.
if (-not (Get-Command tar.exe -ErrorAction SilentlyContinue)) {
	Die 'tar.exe was not found. It ships with Windows 10 1803 and later; on an older build, install Git for Windows and use install.sh in Git Bash.'
}

# --- target ------------------------------------------------------------------
$arch = $env:PROCESSOR_ARCHITECTURE
if ($env:PROCESSOR_ARCHITEW6432) { $arch = $env:PROCESSOR_ARCHITEW6432 }

switch ($arch) {
	'AMD64' { $Target = 'x86_64-pc-windows-msvc' }
	'ARM64' {
		# No aarch64-pc-windows-msvc build is published yet. Windows on ARM runs
		# x64 binaries under emulation, so this works — say so rather than let it
		# look like a native build.
		$Target = 'x86_64-pc-windows-msvc'
		Dim 'no native arm64 build yet — installing the x64 build, which Windows runs under emulation'
	}
	'x86' { Die 'lattice does not publish a 32-bit build' }
	default { Die "unsupported architecture: $arch" }
}

# --- version -----------------------------------------------------------------
function Get-Pin {
	# Deliberately narrow: only a top-level string value.
	$json = Get-Content -Raw -LiteralPath 'lattice.json' | ConvertFrom-Json
	$field = $json.PSObject.Properties['latticeVersion']
	if ($field) { return [string] $field.Value }
	return ''
}

function Get-FirstTag { param($Payload)
	if (-not $Payload) { return '' }
	# /releases/latest answers with one object, the list with an array ordered
	# newest-first.
	$first = if ($Payload -is [array]) { $Payload[0] } else { $Payload }
	if (-not $first) { return '' }
	$field = $first.PSObject.Properties['tag_name']
	if (-not $field) { return '' }
	return ([string] $field.Value) -replace '^v', ''
}

function Get-Json { param([string] $Url)
	return Invoke-RestMethod -Uri $Url -UserAgent 'lattice-installer' -TimeoutSec 30
}

if ($env:LATTICE_VERSION) {
	$Version = $env:LATTICE_VERSION -replace '^v', ''
	$Source = '$env:LATTICE_VERSION'
} elseif (Test-Path -LiteralPath 'lattice.json') {
	$Version = Get-Pin
	if (-not $Version) {
		Die @'
lattice.json has no "latticeVersion". Add the version this repo
       should use, or set $env:LATTICE_VERSION to install a specific one.
'@
	}
	$Version = $Version -replace '^v', ''
	$Source = 'lattice.json'
} else {
	Dim 'no lattice.json here — installing the newest release'
	# /releases/latest is the last *stable* release, so it 404s for a project whose
	# every release so far is a pre-release. That 404 is an expected answer here
	# rather than a failure.
	$Version = ''
	try { $Version = Get-FirstTag (Get-Json $LatestUrl) } catch {}
	if (-not $Version) {
		try { $Version = Get-FirstTag (Get-Json $ListUrl) } catch {}
	}
	if (-not $Version) {
		Die "could not find a release to install`n       tried $LatestUrl`n         and $ListUrl"
	}
	$Source = 'newest release'
	# Read off the version itself rather than off which URL answered, so this says
	# nothing the downloaded artifact does not.
	if ($Version -like '*-*') { Dim "$Version is a pre-release — no stable release yet" }
}

# A version reaches a URL and a filename, so anything but a version stops here.
if ($Version -notmatch '^[0-9][0-9.a-zA-Z+-]*$') {
	Die "'$Version' does not look like a version"
}

$BinDir = '.lattice\bin'
$Versioned = Join-Path $BinDir "lattice-$Version.exe"
$Stable = Join-Path $BinDir 'lattice.exe'
$Asset = "lattice-$Version-$Target.tar.gz"
$Checksums = "lattice-$Version-checksums.txt"

Write-Host 'lattice ' -ForegroundColor White -NoNewline
Write-Host "$Version " -NoNewline
Dim "($Target, from $Source)"

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

# --- download, verify, install ----------------------------------------------
if (Test-Path -LiteralPath $Versioned) {
	Dim 'already downloaded'
} else {
	$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("lattice-install." + [Guid]::NewGuid().ToString('N').Substring(0, 8))
	New-Item -ItemType Directory -Force -Path $Tmp | Out-Null
	try {
		$archive = Join-Path $Tmp $Asset
		Say "> downloading $Asset"
		try {
			Invoke-WebRequest -Uri "$BaseUrl/v$Version/$Asset" -OutFile $archive -UserAgent 'lattice-installer'
		} catch {
			Die "no release asset for this platform: $BaseUrl/v$Version/$Asset"
		}

		$sums = Join-Path $Tmp $Checksums
		try {
			Invoke-WebRequest -Uri "$BaseUrl/v$Version/$Checksums" -OutFile $sums -UserAgent 'lattice-installer'
		} catch {
			Die "could not download $Checksums; refusing to install an unverified binary"
		}

		$expected = ''
		$pattern = '^([0-9a-fA-F]{64})\s+\*?(?:\./)?' + [regex]::Escape($Asset) + '$'
		foreach ($line in Get-Content -LiteralPath $sums) {
			if ($line -match $pattern) {
				$expected = $Matches[1]
				break
			}
		}
		if (-not $expected) { Die "$Checksums does not list $Asset" }

		$actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
		if ($expected -ne $actual) {
			Die "checksum mismatch for $Asset`n       expected $expected`n       actual   $actual"
		}
		Say '+ checksum verified' ([ConsoleColor]::Green)

		& tar.exe -xzf $archive -C $Tmp
		if ($LASTEXITCODE -ne 0) { Die "could not extract $Asset" }

		$extracted = Get-ChildItem -Path $Tmp -Recurse -Filter 'lattice.exe' -File | Select-Object -First 1
		if (-not $extracted) { Die "$Asset contains no lattice.exe" }

		Move-Item -Force -LiteralPath $extracted.FullName -Destination $Versioned
	} finally {
		Remove-Item -Recurse -Force -LiteralPath $Tmp -ErrorAction SilentlyContinue
	}
}

# --- link --------------------------------------------------------------------
# Symlinks need a privilege Windows does not grant by default, so the stable path
# is a copy — the same thing `lattice upgrade` does. A running .exe cannot be
# overwritten but can be renamed out of the way, which is what makes replacing
# the binary you are currently running work at all.
Get-ChildItem -Path $BinDir -Filter '.lattice.old-*.exe' -File -Force -ErrorAction SilentlyContinue |
	Remove-Item -Force -ErrorAction SilentlyContinue

if (Test-Path -LiteralPath $Stable) {
	$evicted = Join-Path $BinDir (".lattice.old-" + [Guid]::NewGuid().ToString('N').Substring(0, 8) + ".exe")
	try {
		Move-Item -Force -LiteralPath $Stable -Destination $evicted
		Remove-Item -Force -LiteralPath $evicted -ErrorAction SilentlyContinue
	} catch {
		Die "could not replace $Stable; close any running lattice and try again"
	}
}
Copy-Item -Force -LiteralPath $Versioned -Destination $Stable

# Machine-local and ephemeral; never a commit.
if (Test-Path -LiteralPath '.gitignore') {
	if (-not (Select-String -LiteralPath '.gitignore' -Pattern '^\.lattice/bin/$' -Quiet)) {
		Add-Content -LiteralPath '.gitignore' -Value '.lattice/bin/'
		Dim 'added .lattice/bin/ to .gitignore'
	}
}

# --- PATH --------------------------------------------------------------------
# Absolute, because an entry in PATH cannot be relative to whatever directory you
# happen to be standing in.
$BinAbs = (Resolve-Path -LiteralPath $BinDir).Path

function Test-OnPath {
	$parts = ($env:Path -split ';') | Where-Object { $_ }
	foreach ($p in $parts) {
		if ($p.TrimEnd('\') -ieq $BinAbs.TrimEnd('\')) { return $true }
	}
	return $false
}

function Confirm-PathEdit {
	if ($AssumeYes) { return $true }
	# Piped input means nothing is there to answer. `irm | iex` does not redirect
	# stdin — the script arrives as a pipeline object — so an ordinary install
	# still gets the prompt.
	if ([Console]::IsInputRedirected -or -not [Environment]::UserInteractive) { return $false }
	try {
		$answer = Read-Host "Add $BinAbs to your user PATH? [Y/n]"
	} catch {
		return $false
	}
	return ($answer -eq '' -or $answer -match '^[Yy]')
}

# SendMessageTimeout rather than a plain broadcast: an unresponsive top-level
# window would otherwise hang the install.
function Publish-EnvironmentChange {
	if (-not ('Lattice.Native' -as [type])) {
		Add-Type -Namespace 'Lattice' -Name 'Native' -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError = true, CharSet = System.Runtime.InteropServices.CharSet.Auto)]
public static extern System.IntPtr SendMessageTimeout(
	System.IntPtr hWnd, uint Msg, System.IntPtr wParam, string lParam,
	uint fuFlags, uint uTimeout, out System.UIntPtr lpdwResult);
'@
	}
	$ignored = [UIntPtr]::Zero
	# HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG, 5s.
	[Lattice.Native]::SendMessageTimeout(
		[IntPtr] 0xffff, 0x1A, [IntPtr]::Zero, 'Environment', 0x2, 5000, [ref] $ignored) | Out-Null
}

$key = $null
$manual = "  to put it on PATH:  `$env:Path = '$BinAbs;' + `$env:Path"
$OnPath = Test-OnPath

if ($OnPath) {
	$PathNote = 'already on PATH'
	$PathNoteDim = $true
} elseif ($NoModifyPath) {
	$PathNote = $manual
	$PathNoteDim = $false
} else {
	# Straight from the registry, and deliberately not through
	# [Environment]::GetEnvironmentVariable: that expands a REG_EXPAND_SZ value, so
	# writing the result back would turn someone's `%USERPROFILE%\bin` entry into a
	# frozen literal path. The value kind is preserved for the same reason. `setx`
	# is not an option either -- it truncates at 1024 characters.
	$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
	$userPath = ''
	$kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
	if ($key) {
		$userPath = [string] $key.GetValue(
			'Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
		try { $kind = $key.GetValueKind('Path') } catch {}
	}
	$already = ($userPath -split ';') | Where-Object { $_.TrimEnd('\') -ieq $BinAbs.TrimEnd('\') }

	if ($already) {
		$PathNote = 'already in your user PATH — open a new shell to use it'
		$PathNoteDim = $true
	} elseif (Confirm-PathEdit) {
		$updated = if ($userPath) { "$BinAbs;$userPath" } else { $BinAbs }
		try {
			if (-not $key) { throw 'HKCU\Environment could not be opened for writing' }
			$key.SetValue('Path', $updated, $kind)
			# A registry write alone is invisible to anything already running,
			# including the Explorer session that new consoles inherit from.
			Publish-EnvironmentChange
			$env:Path = "$BinAbs;$env:Path"
			$PathNote = 'added .lattice\bin to your user PATH — open a new shell to use it'
			$PathNoteDim = $true
		} catch {
			$PathNote = $manual
			$PathNoteDim = $false
		}
	} else {
		$PathNote = $manual
		$PathNoteDim = $false
	}
}

if ($key) { $key.Close() }

# Only a shell that has already picked the entry up can run the bare name.
$Run = if ($OnPath) { 'lattice' } else { '.\.lattice\bin\lattice.exe' }

Say ''
Write-Host '+ ' -ForegroundColor Green -NoNewline
Write-Host "lattice $Version" -ForegroundColor White -NoNewline
Write-Host " installed at $Versioned"
if ($PathNoteDim) { Dim $PathNote } else { Say $PathNote }
Say ''
Say "  run       $Run run build"
Say "  commands  $Run --help"
Say '  remove    rd /s /q .lattice'
if (-not (Test-Path -LiteralPath 'lattice.json')) {
	Say ''
	Say "  This directory has no lattice.json yet. $Run init writes one."
}
