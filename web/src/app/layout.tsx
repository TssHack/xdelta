import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "xdelta — Native, Ultra-Fast Linux Proxy Client (Rust & GTK4)",
  description:
    "Next-generation native Linux proxy client for Xray-Core and Sing-Box. Sub-40 MB RAM, instant launch, Libadwaita GNOME styling, and out-of-the-box pre-bundled core.",
  keywords: [
    "xdelta",
    "xray",
    "sing-box",
    "linux vpn",
    "linux proxy client",
    "vless reality",
    "gtk4",
    "libadwaita",
    "rust",
  ],
  authors: [{ name: "TssHack Team", url: "https://github.com/TssHack/xdelta" }],
  openGraph: {
    title: "xdelta — Native, Ultra-Fast Linux Proxy Client",
    description:
      "Native Rust & GTK4 proxy client for Xray & Sing-Box with pre-bundled core for Debian, Arch, Fedora, and AppImage.",
    url: "https://github.com/TssHack/xdelta",
    siteName: "xdelta",
    images: [
      {
        url: "/screenshots/dashboard.png",
        width: 1200,
        height: 630,
        alt: "xdelta Dashboard",
      },
    ],
    locale: "en_US",
    type: "website",
  },
  icons: {
    icon: "/xdelta-app.svg",
    shortcut: "/xdelta.svg",
    apple: "/xdelta-app.svg",
  },
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html
      lang="en"
      className={`${geistSans.variable} ${geistMono.variable} dark antialiased scroll-smooth`}
    >
      <body className="min-h-screen bg-[#030712] text-[#f8fafc] selection:bg-[#3584e4]/30 selection:text-white">
        {children}
      </body>
    </html>
  );
}
