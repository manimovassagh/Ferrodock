import React, { useState } from "react";
import clsx from "clsx";
import Layout from "@theme/Layout";
import Link from "@docusaurus/Link";
import styles from "./compare.module.css";
import benchmarks from "../data/benchmarks.json";

// Every figure on this page is either measured or taken from the vendor's
// own page, with the source below it. Read on 2026-10-20.
const CHECKED = "October 20, 2026";

const DOCKER_PRICING = "https://www.docker.com/pricing/";
const DOCKER_FAQ = "https://www.docker.com/pricing/faq/";
const ORBSTACK_PRICING = "https://orbstack.dev/pricing";
const ORBSTACK_LICENSE = "https://docs.orbstack.dev/licensing";

// Price per developer per year, billed annually (USD).
const DOCKER_PLANS = {
  Team: { perYear: 15 * 12, label: "Team, $15/user/month" },
  Business: { perYear: 24 * 12, label: "Business, $24/user/month" },
};
const ORBSTACK_PER_YEAR = 96;

// Tools in a fixed order: a tool keeps its place and colour everywhere.
const TOOLS = ["Ferrodock", "Docker Desktop", "OrbStack", "Rancher Desktop", "Podman Desktop"];

// First-time download for an Apple Silicon Mac, in MB (10^6 bytes), with
// what it includes. Only sizes we could check are charted.
const SIZES = [
  {
    tool: "Ferrodock",
    mb: 149.4,
    detail: "31.5 MB app (v1.27.1) + 117.9 MB engine, downloaded on first start",
    source: "https://github.com/manimovassagh/Ferrodock/releases",
  },
  {
    tool: "Rancher Desktop",
    mb: 648.4,
    detail: "v1.24.0 DMG, engine included",
    source: "https://github.com/rancher-sandbox/rancher-desktop/releases/tag/v1.24.0",
  },
  {
    tool: "Podman Desktop",
    mb: 246.6,
    detail: "v1.30.5 DMG; its Podman machine image comes on top, on first start",
    source: "https://github.com/podman-desktop/podman-desktop/releases/tag/v1.30.5",
  },
];

const MEMORY = [
  ["Ferrodock", "4 GB", "Settings → Engine", "/docs/engine"],
  ["Docker Desktop", "Half your Mac's memory", "Docker's settings docs", "https://docs.docker.com/desktop/settings-and-maintenance/settings/"],
  ["OrbStack", "Up to 8 GB, given back when unused", "OrbStack's docs", "https://orbstack.dev/blog/dynamic-memory"],
  ["Rancher Desktop", "2 GB", "its source code", "https://github.com/rancher-sandbox/rancher-desktop/blob/v1.24.0/pkg/rancher-desktop/config/settings.ts"],
  ["Podman Desktop", "4 GB", "its source code", "https://github.com/podman-desktop/podman-desktop/blob/main/extensions/podman/packages/extension/package.json"],
];

const money = (n) =>
  n === 0 ? "Free" : `$${n.toLocaleString("en-US", { maximumFractionDigits: 0 })}`;

/** Horizontal bars, one per tool, Ferrodock highlighted. Values are
 * labelled on the bars; hovering one shows its detail. */
function Bars({ rows, format, label }) {
  const [hover, setHover] = useState(null);
  const max = Math.max(...rows.map((r) => r.value), 1);
  return (
    <div className={styles.bars} role="img" aria-label={label}>
      {rows.map((r) => (
        <div
          key={r.tool}
          className={styles.barRow}
          onMouseEnter={() => setHover(r.tool)}
          onMouseLeave={() => setHover(null)}
          onFocus={() => setHover(r.tool)}
          onBlur={() => setHover(null)}
          tabIndex={0}
        >
          <span className={styles.barLabel}>{r.tool}</span>
          <span className={styles.barTrack}>
            <span
              className={clsx(styles.bar, r.tool === "Ferrodock" && styles.barUs)}
              style={{ width: `${Math.max((r.value / max) * 100, r.value === 0 ? 0 : 1.5)}%` }}
            />
            <span className={styles.barValue}>{format(r.value)}</span>
          </span>
          {hover === r.tool && r.detail && <span className={styles.tooltip}>{r.detail}</span>}
        </div>
      ))}
    </div>
  );
}

function AsTable({ rows, format, column }) {
  return (
    <details className={styles.asTable}>
      <summary>Show as a table</summary>
      <table>
        <thead>
          <tr>
            <th>Tool</th>
            <th>{column}</th>
            <th>Details</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.tool}>
              <td>{r.tool}</td>
              <td>{format(r.value)}</td>
              <td>{r.detail}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </details>
  );
}

function Hero() {
  return (
    <header className={styles.hero}>
      <div className="container">
        <p className={styles.eyebrow}>Ferrodock vs. the alternatives</p>
        <h1 className={styles.title}>
          Free for your whole team.
          <br />
          <span className={styles.accent}>A fraction of the download.</span>
        </h1>
        <p className={styles.lead}>
          What it costs, what you download and what it uses, side by side. Every number here is
          measured or taken from the vendor's own page, with the source next to it.
        </p>
      </div>
    </header>
  );
}

function Pricing() {
  const [developers, setDevelopers] = useState(25);
  const [plan, setPlan] = useState("Team");
  const [large, setLarge] = useState(true);
  const docker = large ? DOCKER_PLANS[plan].perYear : 0;
  const rows = TOOLS.map((tool) => {
    const perDev = { "Docker Desktop": docker, OrbStack: ORBSTACK_PER_YEAR }[tool] ?? 0;
    const detail = {
      Ferrodock: "MIT licensed: free for everyone, companies included",
      "Docker Desktop": large
        ? `${DOCKER_PLANS[plan].label}, billed yearly`
        : "Free under 250 employees and under $10M revenue",
      OrbStack: "Pro, $96/user/year; free for personal use only",
      "Rancher Desktop": "Apache-2.0: free",
      "Podman Desktop": "Apache-2.0: free",
    }[tool];
    return { tool, value: perDev * developers, detail };
  });
  const saved = Math.max(...rows.map((r) => r.value));
  return (
    <section className={styles.section}>
      <div className="container">
        <h2 className={styles.sectionTitle}>What your team pays per year</h2>
        <div className={styles.controls}>
          <label>
            Developers
            <input
              type="range"
              min="1"
              max="500"
              value={developers}
              onChange={(e) => setDevelopers(Number(e.target.value))}
            />
            <output>{developers}</output>
          </label>
          <label>
            Docker plan
            <select value={plan} onChange={(e) => setPlan(e.target.value)}>
              {Object.keys(DOCKER_PLANS).map((name) => (
                <option key={name}>{name}</option>
              ))}
            </select>
          </label>
          <label className={styles.check}>
            <input type="checkbox" checked={large} onChange={(e) => setLarge(e.target.checked)} />
            250+ employees or $10M+ revenue
          </label>
        </div>
        <div className={styles.headline}>
          <span className={styles.headlineValue}>{money(saved)}</span>
          <span className={styles.headlineLabel}>
            a year that {developers === 1 ? "one developer" : `${developers} developers`} don't pay
            with Ferrodock
          </span>
        </div>
        <Bars rows={rows} format={money} label="Yearly cost per tool" />
        <AsTable rows={rows} format={money} column="Per year" />
        <p className={styles.source}>
          Prices billed yearly, in US dollars, checked {CHECKED}: Docker{" "}
          <Link href={DOCKER_PRICING}>pricing</Link> and <Link href={DOCKER_FAQ}>who must pay</Link>;
          OrbStack <Link href={ORBSTACK_PRICING}>pricing</Link> and{" "}
          <Link href={ORBSTACK_LICENSE}>licensing</Link>. Check them before you buy: prices change.
        </p>
      </div>
    </section>
  );
}

function Size() {
  const rows = SIZES.map((s) => ({ tool: s.tool, value: s.mb, detail: s.detail }));
  const mb = (n) => `${n.toLocaleString("en-US", { maximumFractionDigits: 1 })} MB`;
  return (
    <section className={clsx(styles.section, styles.alt)}>
      <div className="container">
        <h2 className={styles.sectionTitle}>What you download</h2>
        <p className={styles.sectionLead}>
          Everything needed to run containers on an Apple Silicon Mac, the first time.
        </p>
        <Bars rows={rows} format={mb} label="Download size per tool" />
        <AsTable rows={rows} format={mb} column="Download" />
        <p className={styles.source}>
          Exact sizes of the release files:{" "}
          {SIZES.map((s, i) => (
            <span key={s.tool}>
              {i > 0 && ", "}
              <Link href={s.source}>{s.tool}</Link>
            </span>
          ))}
          . Docker Desktop's and OrbStack's installers aren't on a page we could check, so they're
          left out rather than guessed.
        </p>
      </div>
    </section>
  );
}

function Memory() {
  return (
    <section className={styles.section}>
      <div className="container">
        <h2 className={styles.sectionTitle}>Memory given to the engine</h2>
        <p className={styles.sectionLead}>
          Each tool runs Linux in a virtual machine on your Mac. This is what it may take by
          default; every tool lets you change it.
        </p>
        <div className={styles.memoryGrid}>
          {MEMORY.map(([tool, value, where, href]) => (
            <div key={tool} className={clsx(styles.memoryCard, tool === "Ferrodock" && styles.memoryUs)}>
              <div className={styles.memoryTool}>{tool}</div>
              <div className={styles.memoryValue}>{value}</div>
              <Link className={styles.memorySource} href={href}>
                {where}
              </Link>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

const METRICS = [
  ["idle_memory_mib", "Memory while idle", (n) => `${n} MiB`],
  ["idle_cpu_percent", "CPU while idle", (n) => `${n}%`],
  ["cold_start_s", "Start until Docker answers", (n) => `${n} s`],
  ["docker_run_ms", "docker run alpine true", (n) => `${n} ms`],
];

function Benchmarks() {
  const runs = benchmarks.runs;
  return (
    <section className={clsx(styles.section, styles.alt)}>
      <div className="container">
        <h2 className={styles.sectionTitle}>Speed and weight</h2>
        <p className={styles.sectionLead}>
          Idle memory and CPU, start time, and how fast a container starts: measured with the same
          script, on the same machine, for every tool.
        </p>
        {runs.length === 0 ? (
          <div className={styles.pending}>
            <h3>Being measured</h3>
            <p>
              We only publish numbers we've measured ourselves, on one machine, with each tool's
              defaults. The script is open, so you can check them, or run it on your own Mac today:
            </p>
            <pre>
              <code>
                git clone https://github.com/manimovassagh/Ferrodock{"\n"}
                Ferrodock/bench/bench.sh "Ferrodock" 'Ferrodock|com.apple.Virtualization.VirtualMachine' 'open -a Ferrodock'
              </code>
            </pre>
            <Link href="https://github.com/manimovassagh/Ferrodock/tree/main/bench">
              How it measures, and the command for each tool →
            </Link>
          </div>
        ) : (
          <>
            <div className={styles.metricGrid}>
              {METRICS.map(([key, title, format]) => {
                const rows = TOOLS.map((tool) => runs.find((r) => r.tool === tool))
                  .filter((r) => r && r[key] !== null && r[key] !== undefined)
                  .map((r) => ({ tool: r.tool, value: r[key] }));
                return (
                  <div key={key}>
                    <h3 className={styles.metricTitle}>{title}</h3>
                    <Bars rows={rows} format={format} label={title} />
                    <AsTable rows={rows} format={format} column={title} />
                  </div>
                );
              })}
            </div>
            <p className={styles.source}>
              Measured on {benchmarks.machine} with{" "}
              <Link href="https://github.com/manimovassagh/Ferrodock/tree/main/bench">
                bench/bench.sh
              </Link>
              ; lower is better.
            </p>
          </>
        )}
      </div>
    </section>
  );
}

export default function Compare() {
  return (
    <Layout
      title="Compare Ferrodock with Docker Desktop, OrbStack, Rancher and Podman"
      description="Price for a team, download size and engine memory of Ferrodock, Docker Desktop, OrbStack, Rancher Desktop and Podman Desktop, with sources."
    >
      <main className={styles.page}>
        <Hero />
        <Pricing />
        <Size />
        <Memory />
        <Benchmarks />
        <section className={styles.final}>
          <div className="container">
            <h2>Try it on your own machine</h2>
            <p>Free, open source, and on macOS, Windows and Linux.</p>
            <Link className={styles.button} to="/">
              Download Ferrodock
            </Link>
          </div>
        </section>
      </main>
    </Layout>
  );
}
