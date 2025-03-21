/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */


const lightCodeTheme = require('prism-react-renderer/themes/github');
const darkCodeTheme = require('prism-react-renderer/themes/dracula');

// With JSDoc @type annotations, IDEs can provide config autocompletion
/** @type {import('@docusaurus/types').DocusaurusConfig} */
(module.exports = {
  title: 'ELP',
  tagline: 'The Erlang Language Platform',
  url: 'https://whatsapp.github.io',
  baseUrl: '/erlang-language-platform/',
  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'throw',
  trailingSlash: true,
  favicon: 'img/elp_icon_color.svg',
  organizationName: 'whatsapp',
  projectName: 'erlang-language-platform',

  presets: [
    [
      'docusaurus-plugin-internaldocs-fb/docusaurus-preset',
      /** @type {import('docusaurus-plugin-internaldocs-fb').PresetOptions} */
      ({
        docs: {
          sidebarPath: require.resolve('./sidebars.js'),
        },
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      }),
    ],
  ],

  plugins: [
    [require.resolve('docusaurus-lunr-search'), {
      excludeRoutes: [
      ]
    }],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      navbar: {
        logo: {
          alt: 'ELP Logo',
          src: 'img/elp_logo_color.svg',
        },
        items: [
          {
            type: 'doc',
            docId: 'get-started/get-started',
            position: 'left',
            label: 'Get Started',
          },
          {
            type: 'doc',
            docId: 'feature-gallery',
            position: 'left',
            label: 'Feature Gallery',
          },
          {
            type: 'doc',
            docId: 'erlang-error-index',
            position: 'left',
            label: 'Erlang Error Index',
          },
          {
            href: 'https://github.com/whatsapp/erlang-language-platform',
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Docs',
            items: [
              {
                label: 'Get Started',
                to: '/docs/get-started',
              },
              {
                label: 'Architecture',
                to: '/docs/architecture',
              },
              {
                label: 'Erlang Error Index',
                to: '/docs/erlang-error-index',
              },
            ],
          },
          {
            title: 'Community',
            items: [
              {
                label: 'GitHub Issues',
                href: 'https://github.com/whatsapp/erlang-language-platform/issues',
              }
            ],
          },
          {
            title: 'More',
            items: [
              {
                label: 'GitHub',
                href: 'https://github.com/whatsapp/erlang-language-platform',
              },
              {
                label: 'Contributing',
                href: 'https://github.com/WhatsApp/erlang-language-platform/blob/main/CONTRIBUTING.md',
              },
              {
                label: 'Code Of Conduct',
                href: 'https://github.com/WhatsApp/erlang-language-platform/blob/main/CODE_OF_CONDUCT.md',
              },
              {
                label: 'Terms of Use',
                href: 'https://github.com/whatsapp/erlang-language-platform#terms-of-use',
              },
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} Meta Platforms, Inc. Built with Docusaurus.`,
      },
      prism: {
        theme: lightCodeTheme,
        darkTheme: darkCodeTheme,
      },
    }),
});
