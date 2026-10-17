import React, { useEffect, useState } from "react";
import Layout from "@theme/Layout";
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

function Window({ src, alt, className }) {
  return (
    <div className={clsx(styles.window, className)}>
      <div className={styles.windowBar}>
        <span />
        <span />
        <span />
      </div>
      <img src={useBaseUrl(src)} alt={alt} loading="lazy" />
    </div>
  );
}

function Hero() {
  const platform = usePlatform();
  return (
    <header className={styles.hero}>
      <div className={styles.glow} aria-hidden />
      <div className={clsx("container", styles.heroInner)}>
        <img className={styles.heroLogo} src={useBaseUrl("/img/logo.png")} alt="" />
        <p className={styles.eyebrow}>Free · Open source · 100% Rust</p>
        <h1 className={styles.title}>
          Docker, at <span className={styles.accent}>native speed</span>.
        </h1>
        <p className={styles.subtitle}>
          Ferrodock is a fast, lightweight desktop app for your containers, images, volumes and
          networks. It's a single native app: no Electron, no web view, and no CPU used while idle.
        </p>
        <div className={styles.ctas}>
          <Link className={clsx(styles.button, styles.primary)} href={RELEASES}>
            Download for {platform}
          </Link>
          <Link className={clsx(styles.button, styles.secondary)} to="/docs">
            Read the docs
          </Link>
        </div>
        <p className={styles.platforms}>
          macOS 11+ (Apple Silicon &amp; Intel) · Windows 10/11 · Linux
        </p>
        <Window src="/img/containers.png" alt="Ferrodock showing a list of containers" className={styles.heroShot} />
      </div>
    </header>
  );
}

const STATS = [
  ["0%", "CPU when idle"],
  ["~10 MB", "download"],
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
    title: "Ports in one click",
    text: "Published ports are links. Click 8080 ↗ and your app opens in the browser.",
    icon: icon(<><path d="M7 17L17 7" /><path d="M8 7h9v9" /></>),
  },
  {
    title: "Works with any engine",
    text: "Docker Desktop, Colima, Rancher Desktop, or a plain dockerd. Ferrodock connects as soon as the engine is up.",
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
    title: "Images, volumes and networks",
    text: "Sizes, mountpoints, subnets, and which containers use each one. Remove what's unused in one step, always with a confirmation first.",
    src: "/img/volumes.png",
    alt: "Volumes page",
  },
  {
    title: "At home on your desktop",
    text: "Light and dark themes follow your system setting. Native widgets and the platform font mean it fits in next to your other apps.",
    src: "/img/light-mode.png",
    alt: "Ferrodock in light mode",
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
  macOS: { cmd: BREW, note: "Or download the DMG from the Releases page and drag Ferrodock to Applications." },
  Windows: {
    cmd: "# Download Ferrodock-<version>-windows-x64.zip, unzip it and run:\n.\\Ferrodock.exe",
    note: "Windows 10 and 11 (x64). If SmartScreen appears, choose More info → Run anyway.",
  },
  Linux: {
    cmd: "tar -xzf Ferrodock-<version>-linux-x64.tar.gz\n./ferrodock",
    note: "x64. Needs a running Docker Engine and access to its socket.",
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
      </div>
    </section>
  );
}

function FinalCta() {
  return (
    <section className={styles.final}>
      <div className={styles.glow} aria-hidden />
      <div className="container">
        <h2>Free, open source, and fast.</h2>
        <p>Download Ferrodock, or star it on GitHub to follow along.</p>
        <div className={styles.ctas}>
          <Link className={clsx(styles.button, styles.primary)} href={RELEASES}>
            Download Ferrodock
          </Link>
          <Link className={clsx(styles.button, styles.secondary)} href={REPO}>
            ★ Star on GitHub
          </Link>
        </div>
      </div>
    </section>
  );
}

export default function Home() {
  return (
    <Layout
      title="Docker, at native speed"
      description="Ferrodock is a fast, native desktop app for managing Docker, written entirely in Rust. Free and open source for macOS, Windows and Linux."
    >
      <main className={styles.page}>
        <Hero />
        <Stats />
        <Features />
        <Showcase />
        <Why />
        <Install />
        <FinalCta />
      </main>
    </Layout>
  );
}
