"use client";

import React, { useState } from "react";
import Image from "next/image";
import {
  ShieldCheck,
  Zap,
  Cpu,
  Layers,
  Terminal,
  Download,
  Check,
  Copy,
  ExternalLink,
  ChevronRight,
  Activity,
  Sparkles,
  Sliders,
  Star,
  Flame,
  ChevronDown,
  HelpCircle,
  ArrowUpRight,
  Laptop,
} from "lucide-react";

function GithubIcon({ className = "w-4 h-4" }: { className?: string }) {
  return (
    <svg
      role="img"
      viewBox="0 0 24 24"
      fill="currentColor"
      className={className}
      xmlns="http://www.w3.org/2000/svg"
    >
      <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
    </svg>
  );
}

export default function Home() {
  const [copiedTab, setCopiedTab] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<"arch" | "debian" | "fedora" | "appimage">("arch");
  const [activeScreenshot, setActiveScreenshot] = useState<"dashboard" | "servers" | "settings">("dashboard");
  const [openFaq, setOpenFaq] = useState<number | null>(0);

  const installSnippets = {
    arch: `# Install pre-bundled Arch package directly (Zero external deps)\nsudo pacman -U https://github.com/TssHack/xdelta/releases/download/v0.1.0/xdelta-0.1.0-1-x86_64.pkg.tar.zst`,
    debian: `# Download and install Debian/Ubuntu package with bundled core\ncurl -sLO https://github.com/TssHack/xdelta/releases/download/v0.1.0/xdelta_0.1.0_amd64.deb\nsudo apt install ./xdelta_0.1.0_amd64.deb`,
    fedora: `# Install on Fedora / RHEL / openSUSE\nsudo dnf install https://github.com/TssHack/xdelta/releases/download/v0.1.0/xdelta-0.1.0-1.x86_64.rpm`,
    appimage: `# Run anywhere on Linux without root or installation\ncurl -sLO https://github.com/TssHack/xdelta/releases/download/v0.1.0/xdelta-0.1.0-x86_64.AppImage\nchmod +x xdelta-0.1.0-x86_64.AppImage\n./xdelta-0.1.0-x86_64.AppImage`,
  };

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedTab(id);
    setTimeout(() => setCopiedTab(null), 2000);
  };

  const faqs = [
    {
      q: "Does xdelta require Xray or Sing-box to be installed separately?",
      a: "No! Official release packages (.deb, .rpm, .pkg.tar.zst, and .AppImage) come pre-bundled with the official Xray-core binary and GeoIP/GeoSite databases. If you build from source or install on a bare system, xdelta includes an automatic in-app 1-click core downloader inside the Settings view.",
    },
    {
      q: "Why Rust + GTK4 instead of Electron or Python?",
      a: "Electron clients typically consume 300–500 MB of RAM on Linux and clash with modern desktop environments. xdelta is compiled natively to machine code in Rust, boots in less than 200 ms, consumes under 40 MB of RAM, and matches system accent colors, dark mode, and fractional scaling natively.",
    },
    {
      q: "How does the Layer-3 TUN mode work without breaking local networking?",
      a: "xdelta uses dual /1 routing table boundaries (0.0.0.0/1 and 128.0.0.0/1) with physical gateway pinning to prevent connection loops. It automatically applies systemd-resolved DNS protection against DNS poisoning and cleanly reverts routing upon disconnect.",
    },
    {
      q: "Is domestic traffic bypass (Bypass Iran & LAN) supported?",
      a: "Yes. By utilizing geosite:category-ir and geoip:ir rules alongside custom direct domain filters, traffic destined for domestic services and local LANs bypasses the proxy entirely, keeping banking and local intranet speeds unhindered.",
    },
  ];

  return (
    <div className="relative min-h-screen bg-[var(--md-sys-color-surface)] text-[var(--md-sys-color-on-surface)] selection:bg-[var(--md-sys-color-secondary)]/30 selection:text-white overflow-x-hidden">
      {/* 
        ========================================================================
        MD3 ATMOSPHERIC WALLPAPER & BLOOM LAYER
        ========================================================================
      */}
      <div className="fixed inset-0 pointer-events-none z-0">
        <Image
          src="/wallpaper.png"
          alt="xdelta Atmospheric Cyber Wallpaper"
          fill
          priority
          className="object-cover object-top opacity-35 filter saturate-[1.12] contrast-[1.05]"
        />
        {/* MD3 Surface Scrim */}
        <div className="absolute inset-0 md3-wallpaper-scrim" />
        {/* MD3 Rim Light Ambient Bloom (Top-Left bias from artwork) */}
        <div className="absolute inset-0 md3-ambient-bloom" />
        {/* MD3 Star Accent Warm Bloom */}
        <div className="absolute inset-0 md3-star-bloom" />
      </div>

      {/* 
        ========================================================================
        1. MD3 TOP APP BAR
        ========================================================================
      */}
      <header className="sticky top-0 z-50 w-full border-b border-[var(--md-sys-color-outline-variant)] bg-[var(--md-sys-color-surface-container-low)]/80 backdrop-blur-xl">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 h-16 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="relative flex items-center justify-center w-10 h-10 rounded-[12px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)] border border-[var(--md-sys-color-primary)]/30 shadow-sm md3-glow-primary">
              <Image
                src="/xdelta.svg"
                alt="xdelta Logo"
                width={24}
                height={24}
                className="w-5 h-5 drop-shadow-[0_0_10px_rgba(126,229,255,0.7)]"
              />
            </div>
            <div className="flex items-center gap-2">
              <span className="font-bold text-lg tracking-tight text-white">
                xdelta
              </span>
              <span className="text-[11px] font-bold px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)] border border-[var(--md-sys-color-tertiary)]/30 flex items-center gap-1 shadow-sm">
                <Star className="w-2.5 h-2.5 fill-[var(--md-sys-color-tertiary)] text-[var(--md-sys-color-tertiary)]" />
                v0.1.0
              </span>
            </div>
          </div>

          <nav className="hidden md:flex items-center gap-7 text-sm font-medium text-[var(--md-sys-color-on-surface-variant)]">
            <a
              href="#features"
              className="hover:text-[var(--md-sys-color-primary)] transition-colors duration-200"
            >
              Features
            </a>
            <a
              href="#showcase"
              className="hover:text-[var(--md-sys-color-primary)] transition-colors duration-200"
            >
              Interface
            </a>
            <a
              href="#protocols"
              className="hover:text-[var(--md-sys-color-primary)] transition-colors duration-200"
            >
              Protocols
            </a>
            <a
              href="#downloads"
              className="hover:text-[var(--md-sys-color-primary)] transition-colors duration-200"
            >
              Downloads
            </a>
            <a
              href="#faq"
              className="hover:text-[var(--md-sys-color-primary)] transition-colors duration-200"
            >
              FAQ
            </a>
            <a
              href="https://github.com/TssHack/xdelta#readme"
              target="_blank"
              rel="noreferrer"
              className="hover:text-[var(--md-sys-color-primary)] transition-colors duration-200 flex items-center gap-1 text-[var(--md-sys-color-on-surface-variant)]"
            >
              Docs <ExternalLink className="w-3.5 h-3.5 opacity-60" />
            </a>
          </nav>

          <div className="flex items-center gap-3">
            <a
              href="https://github.com/TssHack/xdelta"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-2 px-3.5 py-2 rounded-full border border-[var(--md-sys-color-outline-variant)] hover:border-[var(--md-sys-color-outline)] bg-[var(--md-sys-color-surface-container)] hover:bg-[var(--md-sys-color-surface-container-high)] text-xs font-medium text-[var(--md-sys-color-on-surface)] transition-all duration-200"
            >
              <GithubIcon className="w-4 h-4 text-[var(--md-sys-color-on-surface-variant)]" />
              <span className="hidden sm:inline">GitHub</span>
            </a>
            <a
              href="#downloads"
              className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] hover:brightness-110 text-xs font-bold shadow-md md3-glow-primary transition-all duration-200 hover:scale-[1.02] active:scale-[0.98]"
            >
              <Download className="w-3.5 h-3.5" />
              <span>Get xdelta</span>
            </a>
          </div>
        </div>
      </header>

      {/* 
        ========================================================================
        2. HERO SECTION
        ========================================================================
      */}
      <section className="relative z-10 pt-16 pb-20 md:pt-24 md:pb-28 px-4 sm:px-6 lg:px-8 max-w-7xl mx-auto flex flex-col items-center text-center">
        {/* MD3 Assist Chip with Pulsing Star */}
        <div className="inline-flex items-center gap-2.5 px-4 py-1.5 rounded-full border border-[var(--md-sys-color-primary)]/30 bg-[var(--md-sys-color-surface-container)] text-[var(--md-sys-color-primary)] text-xs font-medium mb-8 shadow-sm backdrop-blur-md">
          <span className="flex h-2 w-2 relative">
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-[var(--md-sys-color-primary)] opacity-75" />
            <span className="relative inline-flex rounded-full h-2 w-2 bg-[var(--md-sys-color-secondary)]" />
          </span>
          <span className="text-[var(--md-sys-color-on-surface)] font-medium">
            Native Linux Anti-Censorship • <strong className="text-white">Zero External Setup</strong>
          </span>
          <span className="text-[var(--md-sys-color-tertiary)] flex items-center gap-1 font-bold border-l border-[var(--md-sys-color-outline-variant)] pl-2.5">
            <Flame className="w-3.5 h-3.5 fill-[var(--md-sys-color-tertiary)] text-[var(--md-sys-color-tertiary)]" />
            v0.1.0 Ready
          </span>
        </div>

        {/* MD3 Display Headline */}
        <h1 className="text-4xl sm:text-6xl md:text-7xl font-extrabold tracking-tight max-w-4xl leading-[1.08] mb-6">
          Lightweight, Native Proxy Client for{" "}
          <span className="bg-gradient-to-r from-white via-[var(--md-sys-color-primary)] to-[var(--md-sys-color-secondary)] bg-clip-text text-transparent drop-shadow-[0_0_35px_rgba(126,229,255,0.4)]">
            Linux Desktop
          </span>
        </h1>

        {/* MD3 Body Large */}
        <p className="text-base sm:text-lg md:text-xl text-[var(--md-sys-color-on-surface-variant)] max-w-2xl font-normal leading-relaxed mb-10">
          Crafted with pure <strong className="text-white">Rust</strong> and{" "}
          <strong className="text-white">GTK4 / Libadwaita</strong>. Powered by{" "}
          <strong>Xray-Core</strong> and <strong>Sing-Box</strong> with sub-40 MB memory, instant startup,
          and pre-bundled zero-setup packages for every distribution.
        </p>

        {/* MD3 Action Buttons */}
        <div className="flex flex-wrap items-center justify-center gap-4 mb-14">
          <a
            href="#downloads"
            className="inline-flex items-center gap-2.5 px-8 py-3.5 rounded-full bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] hover:brightness-110 font-bold text-sm shadow-xl md3-glow-primary transition-all duration-200 hover:-translate-y-0.5 active:translate-y-0"
          >
            <Download className="w-4 h-4" />
            <span>Download for Linux</span>
          </a>
          <a
            href="https://github.com/TssHack/xdelta"
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-2 px-7 py-3.5 rounded-full border border-[var(--md-sys-color-outline)] hover:border-[var(--md-sys-color-primary)] bg-[var(--md-sys-color-surface-container)] hover:bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface)] font-semibold text-sm transition-all duration-200 hover:-translate-y-0.5 active:translate-y-0 backdrop-blur-md"
          >
            <GithubIcon className="w-4 h-4 text-[var(--md-sys-color-on-surface-variant)]" />
            <span>Star on GitHub</span>
          </a>
        </div>

        {/* MD3 Terminal Container (Interactive Segmented Tabs) */}
        <div className="w-full max-w-2xl md3-surface-container md3-rim-light rounded-[28px] overflow-hidden shadow-2xl mb-16 border border-[var(--md-sys-color-outline-variant)]">
          <div className="flex items-center justify-between px-5 py-3 border-b border-[var(--md-sys-color-outline-variant)] bg-[var(--md-sys-color-surface-container-high)]/60">
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-rose-500/80" />
              <div className="w-3 h-3 rounded-full bg-amber-500/80" />
              <div className="w-3 h-3 rounded-full bg-emerald-500/80" />
              <span className="ml-2 text-xs font-mono text-[var(--md-sys-color-primary)]">Quick Install</span>
            </div>

            {/* MD3 Filter Chips */}
            <div className="flex items-center gap-1 p-0.5 rounded-full bg-[var(--md-sys-color-surface-container-lowest)] border border-[var(--md-sys-color-outline-variant)] text-[11px] font-medium">
              {(["arch", "debian", "fedora", "appimage"] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setActiveTab(tab)}
                  className={`px-3 py-1 rounded-full capitalize transition-all duration-150 ${
                    activeTab === tab
                      ? "bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] font-bold shadow-sm"
                      : "text-[var(--md-sys-color-on-surface-variant)] hover:text-white"
                  }`}
                >
                  {tab === "arch"
                    ? "Arch / Cachy"
                    : tab === "debian"
                    ? "Debian / Ubuntu"
                    : tab === "fedora"
                    ? "Fedora"
                    : "AppImage"}
                </button>
              ))}
            </div>
          </div>

          <div className="p-5 bg-[var(--md-sys-color-surface-container-lowest)]/95 font-mono text-xs sm:text-sm text-left flex items-start justify-between gap-4">
            <pre className="text-[var(--md-sys-color-primary)] overflow-x-auto whitespace-pre-wrap leading-relaxed">
              <code>{installSnippets[activeTab]}</code>
            </pre>
            <button
              onClick={() => copyToClipboard(installSnippets[activeTab], activeTab)}
              className="p-2.5 rounded-full bg-[var(--md-sys-color-surface-container-high)] hover:bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface-variant)] hover:text-white transition-all duration-150 shrink-0 border border-[var(--md-sys-color-outline-variant)]"
              title="Copy snippet"
            >
              {copiedTab === activeTab ? (
                <Check className="w-4 h-4 text-emerald-400" />
              ) : (
                <Copy className="w-4 h-4" />
              )}
            </button>
          </div>
        </div>

        {/* MD3 Metrics Grid (Tonal Elevation Level 1) */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 w-full max-w-4xl text-left">
          <div className="md3-surface-low md3-rim-light p-5 rounded-[20px] md3-state-layer">
            <div className="text-2xl sm:text-3xl font-extrabold text-[var(--md-sys-color-primary)] mb-1">&lt; 40 MB</div>
            <div className="text-xs font-semibold text-[var(--md-sys-color-on-surface)]">Idle RAM Footprint</div>
            <div className="text-[11px] text-[var(--md-sys-color-on-surface-variant)] mt-1">10x lighter than Electron</div>
          </div>
          <div className="md3-surface-low md3-rim-light p-5 rounded-[20px] md3-state-layer">
            <div className="text-2xl sm:text-3xl font-extrabold text-emerald-400 mb-1">&lt; 200 ms</div>
            <div className="text-xs font-semibold text-[var(--md-sys-color-on-surface)]">Cold Startup Time</div>
            <div className="text-[11px] text-[var(--md-sys-color-on-surface-variant)] mt-1">Compiled native binary</div>
          </div>
          <div className="md3-surface-low md3-rim-light p-5 rounded-[20px] md3-state-layer">
            <div className="text-2xl sm:text-3xl font-extrabold text-[var(--md-sys-color-secondary)] mb-1">Dual-Core</div>
            <div className="text-xs font-semibold text-[var(--md-sys-color-on-surface)]">Xray &amp; Sing-Box</div>
            <div className="text-[11px] text-[var(--md-sys-color-on-surface-variant)] mt-1">VLESS Reality, Hy2, TUIC</div>
          </div>
          <div className="md3-surface-low md3-rim-light p-5 rounded-[20px] md3-state-layer">
            <div className="text-2xl sm:text-3xl font-extrabold text-[var(--md-sys-color-tertiary)] mb-1">0 Deps</div>
            <div className="text-xs font-semibold text-[var(--md-sys-color-on-surface)]">Pre-bundled Core</div>
            <div className="text-[11px] text-[var(--md-sys-color-on-surface-variant)] mt-1">Works out-of-the-box</div>
          </div>
        </div>
      </section>

      {/* 
        ========================================================================
        3. MD3 SHOWCASE / SCREENSHOTS SECTION
        ========================================================================
      */}
      <section id="showcase" className="relative z-10 py-24 border-t border-[var(--md-sys-color-outline-variant)] bg-[var(--md-sys-color-surface-container-lowest)]/60 backdrop-blur-sm">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 text-center">
          <div className="inline-flex items-center gap-2 px-4 py-1.5 rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] border border-[var(--md-sys-color-secondary)]/30 text-xs font-bold uppercase tracking-wider mb-4 shadow-sm">
            <Sparkles className="w-3.5 h-3.5 text-[var(--md-sys-color-primary)]" />
            <span>Modern Libadwaita Interface</span>
          </div>
          <h2 className="text-3xl sm:text-4xl font-extrabold tracking-tight mb-4 text-white">
            Designed to Feel Native on GNOME &amp; Modern Linux
          </h2>
          <p className="text-[var(--md-sys-color-on-surface-variant)] max-w-2xl mx-auto text-sm sm:text-base mb-8 leading-relaxed">
            Follows GNOME Human Interface Guidelines (HIG) with neon glowing power buttons,
            real-time traffic telemetry, left-sidebar navigation, and custom wallpaper themes.
          </p>

          {/* MD3 Segmented Button Group */}
          <div className="inline-flex p-1 rounded-full bg-[var(--md-sys-color-surface-container-high)] border border-[var(--md-sys-color-outline-variant)] mb-10 text-xs sm:text-sm font-medium shadow-inner">
            <button
              onClick={() => setActiveScreenshot("dashboard")}
              className={`px-5 py-2 rounded-full transition-all duration-200 ${
                activeScreenshot === "dashboard"
                  ? "bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] font-bold shadow-md md3-glow-primary"
                  : "text-[var(--md-sys-color-on-surface-variant)] hover:text-white"
              }`}
            >
              Dashboard &amp; Telemetry
            </button>
            <button
              onClick={() => setActiveScreenshot("servers")}
              className={`px-5 py-2 rounded-full transition-all duration-200 ${
                activeScreenshot === "servers"
                  ? "bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] font-bold shadow-md md3-glow-primary"
                  : "text-[var(--md-sys-color-on-surface-variant)] hover:text-white"
              }`}
            >
              Servers &amp; Live Ping
            </button>
            <button
              onClick={() => setActiveScreenshot("settings")}
              className={`px-5 py-2 rounded-full transition-all duration-200 ${
                activeScreenshot === "settings"
                  ? "bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] font-bold shadow-md md3-glow-primary"
                  : "text-[var(--md-sys-color-on-surface-variant)] hover:text-white"
              }`}
            >
              Settings &amp; Appearance
            </button>
          </div>

          {/* App Window Frame Mockup with MD3 Extra-Large Corners (28px) */}
          <div className="max-w-5xl mx-auto rounded-[28px] md3-surface-container md3-rim-light p-2 sm:p-3 border border-[var(--md-sys-color-outline)] shadow-2xl md3-glow-primary transition-all duration-300">
            <div className="rounded-[20px] overflow-hidden bg-slate-950 border border-white/5 relative aspect-[16/10]">
              <Image
                src={`/screenshots/${activeScreenshot}.png`}
                alt={`xdelta ${activeScreenshot}`}
                fill
                className="object-cover object-top transition-all duration-300"
                priority
              />
            </div>
          </div>
        </div>
      </section>

      {/* 
        ========================================================================
        4. MD3 CARDS & BENTO GRID FEATURES
        ========================================================================
      */}
      <section id="features" className="relative z-10 py-24 max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="text-center max-w-3xl mx-auto mb-16">
          <div className="inline-flex items-center gap-2 px-3.5 py-1.5 rounded-full bg-emerald-500/10 text-emerald-400 border border-emerald-500/25 text-xs font-bold uppercase tracking-wider mb-4 shadow-sm">
            <Zap className="w-3.5 h-3.5" />
            <span>Modern Architecture</span>
          </div>
          <h2 className="text-3xl sm:text-5xl font-extrabold tracking-tight mb-4 text-white">
            Engineered for Precision and Freedom
          </h2>
          <p className="text-[var(--md-sys-color-on-surface-variant)] text-sm sm:text-base leading-relaxed">
            Every layer of xdelta is built with memory safety, zero-lag UI responsiveness, and high network throughput.
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          {/* Card 1 */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-7 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="w-12 h-12 rounded-[16px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)] flex items-center justify-center mb-6 shadow-sm">
                <Cpu className="w-6 h-6 text-[var(--md-sys-color-primary)]" />
              </div>
              <h3 className="text-lg font-bold text-white mb-2">Zero Electron Overhead</h3>
              <p className="text-[var(--md-sys-color-on-surface-variant)] text-xs sm:text-sm leading-relaxed">
                Compiled directly to machine code in Rust. No heavy Chromium engine, Node.js background
                watchers, or 300+ MB RAM usage. Stays under 40 MB at all times.
              </p>
            </div>
            <div className="mt-6 pt-4 border-t border-[var(--md-sys-color-outline-variant)] text-xs text-[var(--md-sys-color-primary)] font-bold flex items-center gap-1">
              <span>Pure GTK4 + Libadwaita</span>
              <ChevronRight className="w-3.5 h-3.5" />
            </div>
          </div>

          {/* Card 2 */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-7 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="w-12 h-12 rounded-[16px] bg-emerald-500/15 text-emerald-400 flex items-center justify-center mb-6 shadow-sm border border-emerald-500/25">
                <Layers className="w-6 h-6" />
              </div>
              <h3 className="text-lg font-bold text-white mb-2">Pre-Bundled Zero-Setup Core</h3>
              <p className="text-[var(--md-sys-color-on-surface-variant)] text-xs sm:text-sm leading-relaxed">
                No need to search for Xray in Arch AUR or compile it on Debian. Official packages include
                the core and GeoIP databases, plus an in-app 1-click downloader.
              </p>
            </div>
            <div className="mt-6 pt-4 border-t border-[var(--md-sys-color-outline-variant)] text-xs text-emerald-400 font-bold flex items-center gap-1">
              <span>Ready out of the box</span>
              <ChevronRight className="w-3.5 h-3.5" />
            </div>
          </div>

          {/* Card 3 */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-7 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="w-12 h-12 rounded-[16px] bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] flex items-center justify-center mb-6 shadow-sm border border-[var(--md-sys-color-secondary)]/30">
                <ShieldCheck className="w-6 h-6 text-[var(--md-sys-color-secondary)]" />
              </div>
              <h3 className="text-lg font-bold text-white mb-2">Layer-3 TUN &amp; System Proxy</h3>
              <p className="text-[var(--md-sys-color-on-surface-variant)] text-xs sm:text-sm leading-relaxed">
                Dual `/1` kernel routing table integration with anti-DNS poisoning safeguards. Full GNOME
                Desktop proxy automation using Gio GSettings with automatic cleanup.
              </p>
            </div>
            <div className="mt-6 pt-4 border-t border-[var(--md-sys-color-outline-variant)] text-xs text-[var(--md-sys-color-secondary)] font-bold flex items-center gap-1">
              <span>Full OS network capture</span>
              <ChevronRight className="w-3.5 h-3.5" />
            </div>
          </div>

          {/* Card 4 */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-7 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="w-12 h-12 rounded-[16px] bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)] flex items-center justify-center mb-6 shadow-sm border border-[var(--md-sys-color-tertiary)]/30">
                <Activity className="w-6 h-6 text-[var(--md-sys-color-tertiary)]" />
              </div>
              <h3 className="text-lg font-bold text-white mb-2">Real TCP &amp; HTTP Prober</h3>
              <p className="text-[var(--md-sys-color-on-surface-variant)] text-xs sm:text-sm leading-relaxed">
                ICMP pings often lie. xdelta executes concurrent asynchronous TCP socket handshakes and real
                HTTP verification to guarantee server viability before you connect.
              </p>
            </div>
            <div className="mt-6 pt-4 border-t border-[var(--md-sys-color-outline-variant)] text-xs text-[var(--md-sys-color-tertiary)] font-bold flex items-center gap-1">
              <span>Tokio async worker pool</span>
              <ChevronRight className="w-3.5 h-3.5" />
            </div>
          </div>

          {/* Card 5 */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-7 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="w-12 h-12 rounded-[16px] bg-amber-500/15 text-amber-400 flex items-center justify-center mb-6 shadow-sm border border-amber-500/25">
                <Terminal className="w-6 h-6" />
              </div>
              <h3 className="text-lg font-bold text-white mb-2">Live Diagnostics &amp; Logs</h3>
              <p className="text-[var(--md-sys-color-on-surface-variant)] text-xs sm:text-sm leading-relaxed">
                Dedicated syntax-highlighted diagnostics terminal. Stream stdout/stderr from Xray in
                real-time, filter by error level, and copy logs with a single click.
              </p>
            </div>
            <div className="mt-6 pt-4 border-t border-[var(--md-sys-color-outline-variant)] text-xs text-amber-400 font-bold flex items-center gap-1">
              <span>Zero UI blocking</span>
              <ChevronRight className="w-3.5 h-3.5" />
            </div>
          </div>

          {/* Card 6 */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-7 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="w-12 h-12 rounded-[16px] bg-rose-500/15 text-rose-400 flex items-center justify-center mb-6 shadow-sm border border-rose-500/25">
                <Sliders className="w-6 h-6" />
              </div>
              <h3 className="text-lg font-bold text-white mb-2">Smart Bypass &amp; Adblock</h3>
              <p className="text-[var(--md-sys-color-on-surface-variant)] text-xs sm:text-sm leading-relaxed">
                Intelligent domestic domain routing via `geosite:category-ir` and `geoip:ir`. Keep banking
                and local services blazing fast without leaking proxy credentials.
              </p>
            </div>
            <div className="mt-6 pt-4 border-t border-[var(--md-sys-color-outline-variant)] text-xs text-rose-400 font-bold flex items-center gap-1">
              <span>Configurable routing engine</span>
              <ChevronRight className="w-3.5 h-3.5" />
            </div>
          </div>
        </div>
      </section>

      {/* 
        ========================================================================
        5. PROTOCOL MATRIX TABLE (MD3 DATA DISPLAY)
        ========================================================================
      */}
      <section id="protocols" className="relative z-10 py-20 border-t border-[var(--md-sys-color-outline-variant)] bg-[var(--md-sys-color-surface-container-low)]/40 backdrop-blur-sm">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="text-center max-w-2xl mx-auto mb-12">
            <h2 className="text-3xl font-extrabold tracking-tight mb-3 text-white">
              Comprehensive Protocol Matrix
            </h2>
            <p className="text-[var(--md-sys-color-on-surface-variant)] text-sm">
              Universal URI parser compatible with VLESS Reality, VMess, Trojan, and Shadowsocks standards.
            </p>
          </div>

          <div className="max-w-4xl mx-auto overflow-hidden rounded-[24px] border border-[var(--md-sys-color-outline-variant)] md3-surface-container shadow-xl">
            <table className="w-full text-left text-xs sm:text-sm">
              <thead className="border-b border-[var(--md-sys-color-outline-variant)] bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface-variant)] uppercase font-bold text-[11px] tracking-wider">
                <tr>
                  <th className="py-4 px-6">Protocol</th>
                  <th className="py-4 px-6">Transport &amp; Security</th>
                  <th className="py-4 px-6">Orchestrated Engine</th>
                  <th className="py-4 px-6 text-right">Status</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--md-sys-color-outline-variant)] text-[var(--md-sys-color-on-surface)]">
                <tr className="hover:bg-[var(--md-sys-color-primary)]/5 transition-colors">
                  <td className="py-4 px-6 font-bold text-white flex items-center gap-2.5">
                    <span className="w-2.5 h-2.5 rounded-full bg-[var(--md-sys-color-secondary)] shadow-sm" />
                    VLESS
                  </td>
                  <td className="py-4 px-6 text-[var(--md-sys-color-on-surface-variant)]">XTLS Reality, Vision, gRPC, WebSocket, TCP</td>
                  <td className="py-4 px-6">Xray-Core / Sing-Box</td>
                  <td className="py-4 px-6 text-right">
                    <span className="text-[11px] font-bold text-emerald-400 bg-emerald-400/10 px-3 py-1 rounded-full border border-emerald-400/25">
                      Stable
                    </span>
                  </td>
                </tr>
                <tr className="hover:bg-[var(--md-sys-color-primary)]/5 transition-colors">
                  <td className="py-4 px-6 font-bold text-white flex items-center gap-2.5">
                    <span className="w-2.5 h-2.5 rounded-full bg-emerald-400 shadow-sm" />
                    VMess
                  </td>
                  <td className="py-4 px-6 text-[var(--md-sys-color-on-surface-variant)]">WebSocket with custom headers, gRPC, HTTPUpgrade</td>
                  <td className="py-4 px-6">Xray-Core / Sing-Box</td>
                  <td className="py-4 px-6 text-right">
                    <span className="text-[11px] font-bold text-emerald-400 bg-emerald-400/10 px-3 py-1 rounded-full border border-emerald-400/25">
                      Stable
                    </span>
                  </td>
                </tr>
                <tr className="hover:bg-[var(--md-sys-color-primary)]/5 transition-colors">
                  <td className="py-4 px-6 font-bold text-white flex items-center gap-2.5">
                    <span className="w-2.5 h-2.5 rounded-full bg-[var(--md-sys-color-primary)] shadow-sm" />
                    Trojan
                  </td>
                  <td className="py-4 px-6 text-[var(--md-sys-color-on-surface-variant)]">Standard TLS, SNI disguise, ALPN negotiation</td>
                  <td className="py-4 px-6">Xray-Core / Sing-Box</td>
                  <td className="py-4 px-6 text-right">
                    <span className="text-[11px] font-bold text-emerald-400 bg-emerald-400/10 px-3 py-1 rounded-full border border-emerald-400/25">
                      Stable
                    </span>
                  </td>
                </tr>
                <tr className="hover:bg-[var(--md-sys-color-primary)]/5 transition-colors">
                  <td className="py-4 px-6 font-bold text-white flex items-center gap-2.5">
                    <span className="w-2.5 h-2.5 rounded-full bg-[var(--md-sys-color-tertiary)] shadow-sm" />
                    Shadowsocks
                  </td>
                  <td className="py-4 px-6 text-[var(--md-sys-color-on-surface-variant)]">AEAD Ciphers, 2022 Spec (Blake3, Chacha20)</td>
                  <td className="py-4 px-6">Xray-Core / Sing-Box</td>
                  <td className="py-4 px-6 text-right">
                    <span className="text-[11px] font-bold text-emerald-400 bg-emerald-400/10 px-3 py-1 rounded-full border border-emerald-400/25">
                      Stable
                    </span>
                  </td>
                </tr>
                <tr className="hover:bg-[var(--md-sys-color-primary)]/5 transition-colors">
                  <td className="py-4 px-6 font-bold text-white flex items-center gap-2.5">
                    <span className="w-2.5 h-2.5 rounded-full bg-purple-400 shadow-sm" />
                    Hysteria 2 &amp; TUIC
                  </td>
                  <td className="py-4 px-6 text-[var(--md-sys-color-on-surface-variant)]">QUIC-based UDP tunneling, BBR congestion control</td>
                  <td className="py-4 px-6">Sing-Box Engine</td>
                  <td className="py-4 px-6 text-right">
                    <span className="text-[11px] font-bold text-[var(--md-sys-color-primary)] bg-[var(--md-sys-color-primary)]/10 px-3 py-1 rounded-full border border-[var(--md-sys-color-primary)]/25">
                      Planned
                    </span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </section>

      {/* 
        ========================================================================
        6. DISTRIBUTION DOWNLOADS SECTION (MD3 ELEVATED CARDS)
        ========================================================================
      */}
      <section id="downloads" className="relative z-10 py-24 max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="text-center max-w-3xl mx-auto mb-16">
          <div className="inline-flex items-center gap-2 px-3.5 py-1.5 rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] border border-[var(--md-sys-color-secondary)]/30 text-xs font-bold uppercase tracking-wider mb-4 shadow-sm">
            <Download className="w-3.5 h-3.5" />
            <span>Official Linux Releases</span>
          </div>
          <h2 className="text-3xl sm:text-5xl font-extrabold tracking-tight mb-4 text-white">
            Download for Your Linux Distribution
          </h2>
          <p className="text-[var(--md-sys-color-on-surface-variant)] text-sm sm:text-base leading-relaxed">
            Every package is cryptographically signed and pre-bundled with Xray-core for out-of-the-box operation.
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-5 gap-4">
          {/* Arch Linux */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="text-xs font-bold text-[var(--md-sys-color-primary)] uppercase tracking-wider mb-1">
                Arch Linux
              </div>
              <h3 className="font-bold text-white text-base mb-1">CachyOS / Manjaro</h3>
              <p className="text-[11px] text-[var(--md-sys-color-on-surface-variant)] mb-5">Native Pacman package (.pkg.tar.zst)</p>
            </div>
            <a
              href="https://github.com/TssHack/xdelta/releases/download/v0.1.0/xdelta-0.1.0-1-x86_64.pkg.tar.zst"
              className="w-full inline-flex items-center justify-center gap-1.5 py-2.5 px-3 rounded-full bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] hover:brightness-110 text-xs font-bold transition-all shadow-md md3-glow-primary"
            >
              <Download className="w-3.5 h-3.5" />
              <span>Download .zst</span>
            </a>
          </div>

          {/* Debian / Ubuntu */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="text-xs font-bold text-emerald-400 uppercase tracking-wider mb-1">
                Debian / Ubuntu
              </div>
              <h3 className="font-bold text-white text-base mb-1">Mint / Pop!_OS</h3>
              <p className="text-[11px] text-[var(--md-sys-color-on-surface-variant)] mb-5">Debian binary package (.deb)</p>
            </div>
            <a
              href="https://github.com/TssHack/xdelta/releases/download/v0.1.0/xdelta_0.1.0_amd64.deb"
              className="w-full inline-flex items-center justify-center gap-1.5 py-2.5 px-3 rounded-full bg-emerald-500 text-slate-950 hover:brightness-110 text-xs font-bold transition-all shadow-md"
            >
              <Download className="w-3.5 h-3.5" />
              <span>Download .deb</span>
            </a>
          </div>

          {/* Fedora / RHEL */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="text-xs font-bold text-[var(--md-sys-color-secondary)] uppercase tracking-wider mb-1">
                Fedora / RHEL
              </div>
              <h3 className="font-bold text-white text-base mb-1">openSUSE</h3>
              <p className="text-[11px] text-[var(--md-sys-color-on-surface-variant)] mb-5">RedHat binary package (.rpm)</p>
            </div>
            <a
              href="https://github.com/TssHack/xdelta/releases/download/v0.1.0/xdelta-0.1.0-1.x86_64.rpm"
              className="w-full inline-flex items-center justify-center gap-1.5 py-2.5 px-3 rounded-full bg-[var(--md-sys-color-secondary)] text-[var(--md-sys-color-on-secondary)] hover:brightness-110 text-xs font-bold transition-all shadow-md"
            >
              <Download className="w-3.5 h-3.5" />
              <span>Download .rpm</span>
            </a>
          </div>

          {/* Universal AppImage */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="text-xs font-bold text-purple-400 uppercase tracking-wider mb-1">
                Universal Linux
              </div>
              <h3 className="font-bold text-white text-base mb-1">Standalone AppImage</h3>
              <p className="text-[11px] text-[var(--md-sys-color-on-surface-variant)] mb-5">Run without root or installation</p>
            </div>
            <a
              href="https://github.com/TssHack/xdelta/releases/download/v0.1.0/xdelta-0.1.0-x86_64.AppImage"
              className="w-full inline-flex items-center justify-center gap-1.5 py-2.5 px-3 rounded-full bg-purple-500 text-white hover:brightness-110 text-xs font-bold transition-all shadow-md"
            >
              <Download className="w-3.5 h-3.5" />
              <span>Download .AppImage</span>
            </a>
          </div>

          {/* Portable Tarball */}
          <div className="md3-surface-low md3-rim-light md3-state-layer p-5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)] flex flex-col justify-between">
            <div>
              <div className="text-xs font-bold text-[var(--md-sys-color-tertiary)] uppercase tracking-wider mb-1">
                Portable Tarball
              </div>
              <h3 className="font-bold text-white text-base mb-1">Custom / Scripted</h3>
              <p className="text-[11px] text-[var(--md-sys-color-on-surface-variant)] mb-5">Archive with user-space installer</p>
            </div>
            <a
              href="https://github.com/TssHack/xdelta/releases/download/v0.1.0/xdelta-0.1.0-linux-x86_64.tar.gz"
              className="w-full inline-flex items-center justify-center gap-1.5 py-2.5 px-3 rounded-full bg-[var(--md-sys-color-surface-container-high)] hover:bg-[var(--md-sys-color-surface-container-highest)] text-xs font-bold text-white border border-[var(--md-sys-color-outline)] transition-all"
            >
              <Download className="w-3.5 h-3.5" />
              <span>Download .tar.gz</span>
            </a>
          </div>
        </div>

        {/* SHA256 verification note */}
        <div className="mt-8 text-center text-xs text-[var(--md-sys-color-on-surface-variant)]">
          All releases are signed and checksummed. View{" "}
          <a
            href="https://github.com/TssHack/xdelta/releases/download/v0.1.0/SHA256SUMS.txt"
            target="_blank"
            rel="noreferrer"
            className="text-[var(--md-sys-color-primary)] hover:underline font-mono"
          >
            SHA256SUMS.txt
          </a>{" "}
          for integrity verification.
        </div>
      </section>

      {/* 
        ========================================================================
        7. MD3 FAQ ACCORDION
        ========================================================================
      */}
      <section id="faq" className="relative z-10 py-20 border-t border-[var(--md-sys-color-outline-variant)] bg-[var(--md-sys-color-surface-container-low)]/50 backdrop-blur-sm">
        <div className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="text-center mb-12">
            <div className="inline-flex items-center gap-2 px-3.5 py-1.5 rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] border border-[var(--md-sys-color-secondary)]/30 text-xs font-bold uppercase tracking-wider mb-4 shadow-sm">
              <HelpCircle className="w-3.5 h-3.5" />
              <span>Frequently Asked Questions</span>
            </div>
            <h2 className="text-3xl font-extrabold tracking-tight text-white">Got Questions?</h2>
          </div>

          <div className="space-y-4">
            {faqs.map((faq, index) => {
              const isOpen = openFaq === index;
              return (
                <div
                  key={index}
                  className="md3-surface-container md3-rim-light rounded-[20px] overflow-hidden border border-[var(--md-sys-color-outline-variant)] transition-all duration-200"
                >
                  <button
                    onClick={() => setOpenFaq(isOpen ? null : index)}
                    className="w-full px-6 py-4 flex items-center justify-between text-left text-sm sm:text-base font-semibold text-white hover:text-[var(--md-sys-color-primary)] transition-colors"
                  >
                    <span>{faq.q}</span>
                    <ChevronDown
                      className={`w-4 h-4 text-[var(--md-sys-color-primary)] transition-transform duration-200 shrink-0 ml-4 ${
                        isOpen ? "transform rotate-180" : ""
                      }`}
                    />
                  </button>
                  {isOpen && (
                    <div className="px-6 pb-5 text-xs sm:text-sm text-[var(--md-sys-color-on-surface-variant)] leading-relaxed border-t border-[var(--md-sys-color-outline-variant)] pt-3">
                      {faq.a}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </section>

      {/* 
        ========================================================================
        8. MD3 FOOTER
        ========================================================================
      */}
      <footer className="relative z-10 border-t border-[var(--md-sys-color-outline-variant)] py-12 bg-[var(--md-sys-color-surface-container-lowest)]/95">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 flex flex-col sm:flex-row items-center justify-between gap-6">
          <div className="flex items-center gap-3">
            <Image src="/xdelta.svg" alt="xdelta" width={24} height={24} className="w-6 h-6" />
            <span className="font-bold text-sm tracking-tight text-white">xdelta</span>
            <span className="text-xs text-[var(--md-sys-color-on-surface-variant)]">
              © 2026 TssHack. Licensed under GNU GPL-3.0.
            </span>
          </div>

          <div className="flex items-center gap-6 text-xs text-[var(--md-sys-color-on-surface-variant)] font-medium">
            <a
              href="https://github.com/TssHack/xdelta/releases"
              target="_blank"
              rel="noreferrer"
              className="hover:text-[var(--md-sys-color-primary)] transition-colors"
            >
              Releases
            </a>
            <a
              href="https://github.com/TssHack/xdelta/blob/main/docs/SRS_FINAL.md"
              target="_blank"
              rel="noreferrer"
              className="hover:text-[var(--md-sys-color-primary)] transition-colors"
            >
              Architecture SRS
            </a>
            <a
              href="https://github.com/TssHack/xdelta"
              target="_blank"
              rel="noreferrer"
              className="hover:text-[var(--md-sys-color-primary)] transition-colors"
            >
              GitHub Repository
            </a>
          </div>
        </div>
      </footer>

      {/* 
        ========================================================================
        9. MD3 EXTENDED FLOATING ACTION BUTTON (FAB)
        ========================================================================
      */}
      <aside className="fixed bottom-6 right-6 z-40">
        <a
          href="#downloads"
          aria-label="Download xdelta Linux packages"
          className="inline-flex items-center gap-2.5 px-5 py-3 rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)] border border-[var(--md-sys-color-primary)]/40 shadow-2xl md3-glow-primary hover:brightness-110 active:scale-95 transition-all duration-200 font-bold text-xs backdrop-blur-md"
        >
          <Download className="w-4 h-4 text-[var(--md-sys-color-primary)]" />
          <span>Download v0.1.0</span>
        </a>
      </aside>
    </div>
  );
}
