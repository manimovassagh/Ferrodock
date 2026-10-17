// @ts-check
const { themes } = require("prism-react-renderer");

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: "IronStack",
  tagline: "A fast, native desktop app for managing Docker, written entirely in Rust.",
  favicon: "img/logo.png",
  url: "https://manimovassagh.github.io",
  baseUrl: "/IronStack/",
  organizationName: "manimovassagh",
  projectName: "IronStack",
  trailingSlash: false,
  onBrokenLinks: "throw",
  i18n: { defaultLocale: "en", locales: ["en"] },

  presets: [
    [
      "classic",
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          routeBasePath: "/",
          sidebarPath: require.resolve("./sidebars.js"),
          editUrl: "https://github.com/manimovassagh/IronStack/edit/main/website/",
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
        title: "IronStack",
        logo: { alt: "IronStack", src: "img/logo.png" },
        items: [
          { type: "docSidebar", sidebarId: "docs", position: "left", label: "Docs" },
          {
            href: "https://github.com/manimovassagh/IronStack/releases/latest",
            label: "Download",
            position: "right",
          },
          { href: "https://github.com/manimovassagh/IronStack", label: "GitHub", position: "right" },
        ],
      },
      footer: {
        style: "dark",
        links: [
          {
            title: "Docs",
            items: [
              { label: "Installation", to: "/installation" },
              { label: "Using IronStack", to: "/usage" },
              { label: "Architecture", to: "/architecture" },
            ],
          },
          {
            title: "Project",
            items: [
              { label: "GitHub", href: "https://github.com/manimovassagh/IronStack" },
              { label: "Releases", href: "https://github.com/manimovassagh/IronStack/releases" },
              { label: "Issues", href: "https://github.com/manimovassagh/IronStack/issues" },
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} Mani Movassagh. MIT License.`,
      },
      prism: { theme: themes.github, darkTheme: themes.dracula, additionalLanguages: ["rust", "bash", "powershell"] },
    }),
};

module.exports = config;
