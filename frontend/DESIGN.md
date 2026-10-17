# AFSSR Console Design

This document governs the internal gateway console, not a marketing site.
Existing permissions, routing, quota semantics and unsaved-draft protections
must survive visual changes.

## References And Adaptation

Reviewed from [awesome-design-md](https://github.com/VoltAgent/awesome-design-md):

- [Vercel](https://github.com/VoltAgent/awesome-design-md/blob/main/design-md/vercel/DESIGN.md): neutral hierarchy and precise control edges.
- [Linear](https://github.com/VoltAgent/awesome-design-md/blob/main/design-md/linear.app/DESIGN.md): disciplined density and distinct surface levels.
- [Supabase](https://github.com/VoltAgent/awesome-design-md/blob/main/design-md/supabase/DESIGN.md): restrained emerald and developer-oriented clarity.

These community analyses are references, not official brand systems. AFSSR
keeps its own branding, navigation and semantics. Do not import their logos,
proprietary fonts, marketing layouts, negative tracking or display typography.

## Tokens

`src/styles/console.css` is the source of truth, scoped to `workspace-console`.
Body-level tokens intentionally reach teleported dialogs and menus.

| Role | Light | Dark |
| --- | --- | --- |
| Canvas | #ffffff | #141517 |
| Surface | #ffffff | #1b1c1f |
| Raised surface | #ffffff | #232427 |
| Primary text | #18181b | #ededf0 |
| Secondary text | #64646e | #a2a2ad |
| Divider | #e4e4e7 | #303136 |
| Input edge | #d4d4d8 | #45464e |
| Action | #087f6b | #50bfa3 |

Status colors are semantic: green success, amber warning, red failure. Use
independent data colors for chart series. Never make every data point green.

## Typography And Geometry

- Use local system sans with PingFang SC / Noto Sans SC fallbacks. No remote font dependency.
- Page titles: 24px / 600 / 30px; compact mobile titles: 20px / 28px.
- Section titles: 15-16px / 600 / 24px. Body: 14px / 20-22px.
- Metadata: 12-13px / 18-20px. Never lower contrast to hide clutter.
- KPI values: 28px / 600 / 36px; compact mobile: 24px / 32px.
- Use tabular numbers for metrics, money and timestamps; letter spacing is zero.
- Spacing: 4, 8, 12, 16, 20, 24, 32px. Desktop gutters: 32px.
- Controls: 6px radius; framed tools/repeated items: at most 8px.
- Keep command/control geometry stable across hover, loading and selection.

## Surfaces And Controls

- Page sections are open bands, rails and tables. Separate with whitespace and rules.
- Avoid nested cards, arbitrary icon tiles, tinted section headings and decorative gradients.
- Metric bands use dividers; individual infrastructure metrics are plain label/value pairs.
- Shadows belong to controls, menus and dialogs, not every page section.
- Primary commands have a subtle top-edge highlight. Secondary commands have a visible neutral edge.
- Status badges use soft semantic backgrounds, readable text and compact 5px corners.
- Preserve native field semantics, visible focus, disabled states and accessible labels.
- Icon-only commands require a tooltip/title and accessible name.
- Keep navigation text readable; selected items pair a soft accent with a thin edge marker.

## Data And Responsive States

- Never fabricate data to fill a chart. Distinguish loading, no sample and failed sources.
- Show existing source-error warnings. Empty chart regions contain a compact state, not empty axes.
- Missing metrics remain missing, not silently converted to healthy status by styling.
- Mobile: collapse navigation, wrap toolbars, use two-column KPI bands and stacked detail sections.
- Tables may scroll within their container, never expand the document viewport.
- At 320px, headings and primary commands must remain separate and readable.
- Disable nonessential motion under `prefers-reduced-motion`.

## Entitlements

- Keep allocation, family definitions and effective preview in separate views.
- Select default/group/user in a subject rail; preserve drafts while switching.
- Present total budgets first, then family/model sub-pools. Sub-pools are included in totals.
- Personal overrides retain precedence over groups; visual changes must not reinterpret inheritance.
- Read-only administrators may navigate and preview, but not mutate or save.

## Verification

Compare native-size desktop and mobile screenshots, including dark mode,
long values, empty data, focus and read-only states. Run frontend tests,
type checking and production build. Do not deploy production or write
production settings as part of a visual pass.
