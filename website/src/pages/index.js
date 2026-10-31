import React, { useEffect, useState } from "react";
import Layout from "@theme/Layout";
import Head from "@docusaurus/Head";
import Link from "@docusaurus/Link";
import useBaseUrl from "@docusaurus/useBaseUrl";
import clsx from "clsx";
import styles from "./index.module.css";

const RELEASES = "https://github.com/manimovassagh/Ferrodock/releases/latest";
const REPO = "https://github.com/manimovassagh/Ferrodock";
const BREW =
  "brew tap manimovassagh/ferrodock https://github.com/manimovassagh/Ferrodock\nbrew install --cask ferrodock";

/** Which download to offer first, guessed from the visitor's browser. */
function usePlatform() {
  const [platform, setPlatform] = useState("macOS");
  useEffect(() => {
    const ua = navigator.userAgent;
    if (/Windows/i.test(ua)) setPlatform("Windows");
    else if (/Linux/i.test(ua) && !/Android/i.test(ua)) setPlatform("Linux");
  }, []);
  return platform;
}

/** Every platform we build for, and how to recognise its download. */
const PLATFORMS = [
  { name: "macOS", detail: "Apple Silicon & Intel", suffix: "-macos-universal.dmg", kind: ".dmg" },
  { name: "Windows", detail: "Windows 10 & 11 · x64", suffix: "-windows-x64.zip", kind: ".zip" },
  { name: "Linux", detail: "x64", suffix: "-linux-x64.tar.gz", kind: ".tar.gz" },
];

/**
 * Direct download links for the latest release, by platform name, and its
 * version. Until GitHub answers (or if it can't), links go to the
 * Releases page.
 */
function useDownloads() {
  const [release, setRelease] = useState({ version: null, links: {} });
  useEffect(() => {
    fetch("https://api.github.com/repos/manimovassagh/Ferrodock/releases/latest")
      .then((r) => (r.ok ? r.json() : Promise.reject()))
      .then((data) => {
        const links = {};
        for (const p of PLATFORMS) {
          const asset = (data.assets || []).find((a) => a.name.endsWith(p.suffix));
          if (asset) links[p.name] = asset.browser_download_url;
        }
        setRelease({ version: data.tag_name, links });
      })
      .catch(() => {});
  }, []);
  return {
    version: release.version,
    link: (name) => release.links[name] || RELEASES,
  };
}

const OS_ICONS = {
  macOS: (
    <path d="M16.4 12.6c0-2.5 2-3.7 2.1-3.8-1.2-1.7-3-1.9-3.6-2-1.5-.2-3 .9-3.8.9-.8 0-2-.9-3.3-.9-1.7 0-3.3 1-4.2 2.5-1.8 3.1-.5 7.7 1.3 10.2.9 1.2 1.9 2.6 3.2 2.6 1.3-.1 1.8-.8 3.3-.8 1.6 0 2 .8 3.3.8 1.4 0 2.3-1.3 3.1-2.5 1-1.4 1.4-2.8 1.4-2.9 0 0-2.8-1-2.8-4.1zM13.9 5.1c.7-.9 1.2-2 1-3.2-1 0-2.3.7-3 1.6-.7.8-1.3 2-1.1 3.1 1.1.1 2.3-.6 3.1-1.5z" fill="currentColor" />
  ),
  Windows: <path d="M3 5.5 10.5 4.4v7.2H3V5.5zm0 13 7.5 1.1v-7.1H3v6zm8.4 1.2L21 21v-8.5h-9.6v7.2zm0-15.4v7.3H21V3l-9.6 1.3z" fill="currentColor" />,
  Linux: (
    <path d="M12 2c-2 0-3.3 1.7-3.3 4.2 0 1.4.4 2.3.1 3.4-.4 1.4-2.6 3.4-3.3 5.8-.4 1.3 0 2.2.6 2.6-.2.7.1 1.5 1 1.8 1.2.4 2.4-.2 3.4 0 .7.2 1 .7 1.5.7s.8-.5 1.5-.7c1-.2 2.2.4 3.4 0 .9-.3 1.2-1.1 1-1.8.6-.4 1-1.3.6-2.6-.7-2.4-2.9-4.4-3.3-5.8-.3-1.1.1-2-.1-3.4C15.3 3.7 14 2 12 2zm-1.3 4c.4 0 .6.4.6.9s-.2.9-.6.9-.6-.4-.6-.9.2-.9.6-.9zm2.6 0c.4 0 .6.4.6.9s-.2.9-.6.9-.6-.4-.6-.9.2-.9.6-.9zM12 8.6c.8 0 1.8.5 1.8.9 0 .3-1 .8-1.8.8s-1.8-.5-1.8-.8c0-.4 1-.9 1.8-.9z" fill="currentColor" />
  ),
};

function OsIcon({ name }) {
  return (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden>
      {OS_ICONS[name]}
    </svg>
  );
}

/** One card per platform, the visitor's own first and highlighted. */
function DownloadCards({ downloads, platform, dark }) {
  const ordered = [...PLATFORMS].sort((a, b) => (b.name === platform) - (a.name === platform));
  return (
    <div className={clsx(styles.downloads, dark && styles.downloadsDark)}>
      {ordered.map((p) => (
        <a
          key={p.name}
          className={clsx(styles.download, p.name === platform && styles.downloadMine)}
          href={downloads.link(p.name)}
        >
          <OsIcon name={p.name} />
          <span className={styles.downloadText}>
            <strong>Download for {p.name}</strong>
            <span>
              {p.detail} · {p.kind}
            </span>
          </span>
        </a>
      ))}
    </div>
  );
}

function CopyButton({ text }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className={styles.copy}
      aria-label="Copy to clipboard"
      onClick={() => {
        navigator.clipboard.writeText(text);
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      }}
    >
      {copied ? "Copied" : "Copy"}
    </button>
  );
}

// `animation` plays instead of `src` unless the visitor prefers reduced motion.
function Window({ src, alt, className, animation }) {
  const image = <img src={useBaseUrl(src)} alt={alt} loading="lazy" />;
  const animationUrl = useBaseUrl(animation ?? src);
  return (
    <div className={clsx(styles.window, className)}>
      <div className={styles.windowBar}>
        <span />
        <span />
        <span />
      </div>
      {animation ? (
        <picture>
          <source
            srcSet={animationUrl}
            type="image/webp"
            media="(prefers-reduced-motion: no-preference)"
          />
          {image}
        </picture>
      ) : (
        image
      )}
    </div>
  );
}

function Hero() {
  const platform = usePlatform();
  const downloads = useDownloads();
  return (
    <header className={styles.hero}>
      <div className={styles.glow} aria-hidden />
      <div className={clsx("container", styles.heroInner)}>
        <p className={styles.eyebrow}>Free · Open source · macOS, Windows and Linux</p>
        <h1 className={styles.title}>
          The native container manager
          <br />
          <span className={styles.accent}>Written in Rust</span>
        </h1>
        <p className={styles.subtitle}>
          One fast, lightweight app for your containers, images, volumes and networks, with its
          own Docker engine built in. No Electron, no web view, no subscription, and no CPU used
          while idle.
        </p>
        <DownloadCards downloads={downloads} platform={platform} dark />
        <p className={styles.platforms}>
          {downloads.version ? `Latest: ${downloads.version} · ` : ""}
          <Link href={RELEASES}>All downloads and checksums</Link> ·{" "}
          <Link to="/docs/installation">Installation guide</Link>
        </p>
        <Window
          src="/img/containers.png"
          animation="/img/demo.webp"
          alt="Ferrodock showing containers grouped by Compose project, a container's live logs and stats, the command palette and Quick start"
          className={styles.heroShot}
        />
      </div>
    </header>
  );
}

const STATS = [
  ["0%", "CPU when idle"],
  ["< 35 MB", "download"],
  ["1", "native executable"],
  ["3", "platforms"],
];

function Stats() {
  return (
    <section className={styles.stats}>
      <div className={clsx("container", styles.statsGrid)}>
        {STATS.map(([value, label]) => (
          <div key={label}>
            <div className={styles.statValue}>{value}</div>
            <div className={styles.statLabel}>{label}</div>
          </div>
        ))}
      </div>
    </section>
  );
}

const icon = (d) => (
  <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    {d}
  </svg>
);

const FEATURES = [
  {
    title: "Command palette",
    text: "Press ⌘K and type. Jump to any container, image or volume, or restart a service, without touching the mouse.",
    icon: icon(<><path d="M4 17l6-6-6-6" /><path d="M12 19h8" /></>),
  },
  {
    title: "Live logs and stats",
    text: "Logs stream as they happen, and CPU and memory are charted in real time. They only stream for the container you're looking at.",
    icon: icon(<path d="M3 12h4l3-8 4 16 3-8h4" />),
  },
  {
    title: "Compose projects",
    text: "Containers are grouped by Compose project. Start, stop, restart or remove a whole stack in one click.",
    icon: icon(<><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /></>),
  },
  {
    title: "Free up disk space",
    text: "See exactly where your gigabytes went, then clean up images, volumes and build cache with a preview of what goes.",
    icon: icon(<><ellipse cx="12" cy="5" rx="8" ry="3" /><path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5" /><path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" /></>),
  },
  {
    title: "Scripts and AI tools",
    text: "fdock brings it all to the terminal, with compact JSON for scripts. ferrodock-mcp lets AI tools read logs, start services and tidy up, and asks before it changes anything.",
    icon: icon(<><path d="M4 17l6-6-6-6" /><path d="M12 19h8" /><circle cx="19" cy="6" r="2" /></>),
  },
  {
    title: "Works with any engine",
    text: "Ferrodock's own engine on Apple Silicon Macs, Windows and Linux, or any other Docker engine you run, local or remote. Ferrodock connects as soon as the engine is up.",
    icon: icon(<><path d="M12 2v6" /><path d="M12 16v6" /><circle cx="12" cy="12" r="4" /></>),
  },
];

function Features() {
  return (
    <section className={styles.section}>
      <div className="container">
        <h2 className={styles.sectionTitle}>Everything you reach for, nothing you don't</h2>
        <p className={styles.sectionLead}>
          The everyday Docker work, in a window that opens instantly and stays out of your way.
        </p>
        <div className={styles.featureGrid}>
          {FEATURES.map((f) => (
            <div key={f.title} className={styles.card}>
              <div className={styles.cardIcon}>{f.icon}</div>
              <h3>{f.title}</h3>
              <p>{f.text}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

const SHOWCASE = [
  {
    title: "Logs that keep up",
    text: "Follow output live, filter it as you type, and copy what you need. Switch containers and the stream switches with you.",
    src: "/img/logs.png",
    alt: "Live container logs",
  },
  {
    title: "See what's using your machine",
    text: "CPU and memory charts update every second, with network and disk totals alongside. Charts only run while you're watching.",
    src: "/img/stats.png",
    alt: "Live CPU and memory charts",
  },
  {
    title: "A Linux machine in one click",
    text: "A small Ubuntu, Debian, Fedora, Alpine or Arch with your user, sudo and git. Share a folder and its shell starts right in your code.",
    src: "/img/linux-machines.png",
    alt: "An Ubuntu machine's shell in a shared folder",
  },
  {
    title: "Kubernetes, only when you want it",
    text: "A local k3s cluster in under a minute, ready for kubectl, with worker nodes and your own images loaded in one click. Until you create one, nothing runs.",
    src: "/img/kubernetes.png",
    alt: "A Kubernetes cluster's terminal with kubectl in Ferrodock",
  },
  {
    title: "Win back your disk space",
    text: "See how much images, containers, volumes and build cache take, and free it in one step, with a preview of exactly what goes.",
    src: "/img/disk.png",
    alt: "Disk usage page",
  },
  {
    title: "Images, volumes and networks",
    text: "Sizes, mountpoints, subnets, and which containers use each one. Remove what's unused in one step, always with a confirmation first.",
    src: "/img/volumes.png",
    alt: "Volumes page",
  },
  {
    title: "At home on your desktop",
    text: "A clean light theme by default and a polished dark one, crisp vector icons and the Inter typeface, at any size from 90% to 125%.",
    src: "/img/dark-mode.png",
    alt: "Ferrodock in dark mode",
  },
];

function Showcase() {
  return (
    <section className={clsx(styles.section, styles.showcase)}>
      <div className="container">
        {SHOWCASE.map((s, i) => (
          <div key={s.title} className={clsx(styles.showRow, i % 2 && styles.showRowFlip)}>
            <div className={styles.showText}>
              <h2>{s.title}</h2>
              <p>{s.text}</p>
            </div>
            <Window src={s.src} alt={s.alt} className={styles.showShot} />
          </div>
        ))}
      </div>
    </section>
  );
}

const WHY = [
  ["Native, not a browser", "Built with Rust and egui. There's no bundled browser engine, so it starts fast and uses little memory."],
  ["Silent when idle", "Docker calls run on a background thread and only changes repaint the window. Leave it open all day."],
  ["Yours to keep", "MIT licensed and free for personal, commercial and company use. No account, no telemetry, no subscription."],
];

const YES = "✓";
const COMPARE = {
  tools: ["Ferrodock", "Docker Desktop", "OrbStack", "Rancher Desktop", "Podman Desktop"],
  rows: [
    ["macOS, Windows and Linux", YES, YES, "macOS only", YES, YES],
    ["App built with", "Rust, native", "Electron", "Swift, native", "Electron", "Electron"],
    ["Docker engine included", YES, YES, YES, YES, "Podman"],
    ["Open source", "MIT", "No", "No", "Apache-2.0", "Apache-2.0"],
    ["CLI and MCP server for AI tools, built in", YES, "MCP Toolkit", "No", "No", "No"],
  ],
};

function Compare() {
  return (
    <section className={styles.section}>
      <div className="container">
        <h2 className={styles.sectionTitle}>How it compares</h2>
        <p className={styles.sectionLead}>
          How Ferrodock's design differs from other Docker desktop apps.
        </p>
        <div className={styles.compareWrap}>
          <table className={styles.compare}>
            <thead>
              <tr>
                <th />
                {COMPARE.tools.map((tool, i) => (
                  <th key={tool} className={clsx(i === 0 && styles.compareUs)}>{tool}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {COMPARE.rows.map(([label, ...cells]) => (
                <tr key={label}>
                  <th scope="row">{label}</th>
                  {cells.map((cell, i) => (
                    <td key={i} className={clsx(i === 0 && styles.compareUs, cell === YES && styles.compareYes)}>
                      {cell}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

function Why() {
  return (
    <section className={clsx(styles.section, styles.why)}>
      <div className="container">
        <h2 className={styles.sectionTitle}>Built to be light</h2>
        <div className={styles.whyGrid}>
          {WHY.map(([title, text]) => (
            <div key={title}>
              <h3>{title}</h3>
              <p>{text}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

const INSTALL = {
  macOS: { cmd: BREW, note: "Or download the DMG and drag Ferrodock to Applications. It isn't notarized yet, so allow it once in System Settings → Privacy & Security → Open Anyway." },
  Windows: {
    cmd: "# Download Ferrodock-<version>-windows-x64.zip, unzip it and run:\n.\\Ferrodock.exe",
    note: "Windows 10 and 11 (x64). Runs its own Docker engine through WSL. If SmartScreen appears, choose More info → Run anyway.",
  },
  Linux: {
    cmd: "tar -xzf Ferrodock-<version>-linux-x64.tar.gz\n./ferrodock",
    note: "x64. Sets up its own Docker engine with one click (asks for your admin password once), or uses the one you have.",
  },
};

function Install() {
  const detected = usePlatform();
  const [tab, setTab] = useState(null);
  const current = tab ?? detected;
  return (
    <section className={styles.section} id="install">
      <div className={clsx("container", styles.install)}>
        <h2 className={styles.sectionTitle}>Up and running in a minute</h2>
        <div className={styles.tabs} role="tablist">
          {Object.keys(INSTALL).map((name) => (
            <button
              key={name}
              type="button"
              role="tab"
              aria-selected={current === name}
              className={clsx(styles.tab, current === name && styles.tabActive)}
              onClick={() => setTab(name)}
            >
              {name}
            </button>
          ))}
        </div>
        <div className={styles.terminal}>
          <CopyButton text={INSTALL[current].cmd} />
          <pre>
            {INSTALL[current].cmd.split("\n").map((line) => (
              <div key={line}>
                {!line.startsWith("#") && <span className={styles.prompt}>$ </span>}
                {line}
              </div>
            ))}
          </pre>
        </div>
        <p className={styles.installNote}>
          {INSTALL[current].note} <Link to="/docs/installation">Installation guide →</Link>
        </p>
        <p className={styles.installNote}>
          Or ask your AI assistant: <em>"Install Ferrodock for me, following{" "}
          https://manimovassagh.github.io/Ferrodock/install.md"</em>
        </p>
      </div>
    </section>
  );
}

function FinalCta() {
  const platform = usePlatform();
  const downloads = useDownloads();
  return (
    <section className={styles.final}>
      <div className={styles.glow} aria-hidden />
      <div className="container">
        <h2>Free, open source, and fast</h2>
        <p>Download Ferrodock for your computer, or star it on GitHub to follow along.</p>
        <DownloadCards downloads={downloads} platform={platform} dark />
        <div className={styles.ctas} style={{ marginTop: "1.5rem" }}>
          <Link className={clsx(styles.button, styles.secondary)} href={REPO}>
            ★ Star on GitHub
          </Link>
        </div>
      </div>
    </section>
  );
}

// What search engines show as the app: name, platforms, price, license.
const SOFTWARE = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "Ferrodock",
  description: "The native container manager, written in Rust",
  applicationCategory: "DeveloperApplication",
  operatingSystem: "macOS 11+, Windows 10 and 11, Linux",
  offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
  license: "https://opensource.org/licenses/MIT",
  url: "https://manimovassagh.github.io/Ferrodock/",
  downloadUrl: "https://github.com/manimovassagh/Ferrodock/releases/latest",
  codeRepository: "https://github.com/manimovassagh/Ferrodock",
};

export default function Home() {
  return (
    <Layout
      title="The native container manager, written in Rust"
      description="Ferrodock is a free, open-source desktop app for Docker on macOS, Windows and Linux, written in Rust, with its own Docker engine, a CLI and an MCP server for AI tools."
    >
      <Head>
        <script type="application/ld+json">{JSON.stringify(SOFTWARE)}</script>
      </Head>
      <main className={styles.page}>
        <Hero />
        <Stats />
        <Features />
        <Showcase />
        <Why />
        <Compare />
        <Install />
        <FinalCta />
      </main>
    </Layout>
  );
}
