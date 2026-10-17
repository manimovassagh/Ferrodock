// @ts-check
const { themes } = require("prism-react-renderer");

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: "Ferrodock",
  tagline: "A fast, native desktop app for managing Docker, written entirely in Rust.",
  favicon: "img/logo.png",
  url: "https://manimovassagh.github.io",
  baseUrl: "/Ferrodock/",
  organizationName: "manimovassagh",
  projectName: "Ferrodock",
  trailingSlash: false,
  onBrokenLinks: "throw",
  i18n: { defaultLocale: "en", locales: ["en"] },
  markdown: { mermaid: true },
  themes: ["@docusaurus/theme-mermaid"],

  presets: [
    [
      "classic",
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          routeBasePath: "/",
          sidebarPath: require.resolve("./sidebars.js"),
          editUrl: "https://github.com/manimovassagh/Ferrodock/edit/main/website/",
        },
        blog: false,
        theme: { customCss: require.resolve("./src/css/custom.css") },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      colorMode: { respectPrefersColorScheme: true },
      navbar: {
        title: "Ferrodock",
        logo: { alt: "Ferrodock", src: "img/logo.png" },
        items: [
          { type: "docSidebar", sidebarId: "docs", position: "left", label: "Docs" },
          {
            href: "https://github.com/manimovassagh/Ferrodock/releases/latest",
            label: "Download",
            position: "right",
          },
          { href: "https://github.com/manimovassagh/Ferrodock", label: "GitHub", position: "right" },
        ],
      },
      footer: {
        style: "dark",
        links: [
          {
            title: "Docs",
            items: [
              { label: "Installation", to: "/installation" },
              { label: "Using Ferrodock", to: "/usage" },
              { label: "Architecture", to: "/architecture" },
            ],
          },
          {
            title: "Project",
            items: [
              { label: "GitHub", href: "https://github.com/manimovassagh/Ferrodock" },
              { label: "Releases", href: "https://github.com/manimovassagh/Ferrodock/releases" },
              { label: "Issues", href: "https://github.com/manimovassagh/Ferrodock/issues" },
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} Mani Movassagh. MIT License.`,
      },
      prism: { theme: themes.github, darkTheme: themes.dracula, additionalLanguages: ["rust", "bash", "powershell"] },
    }),
};

module.exports = config;
