---
name: Aether Terminal
colors:
  surface: '#131313'
  surface-dim: '#131313'
  surface-bright: '#3a3939'
  surface-container-lowest: '#0e0e0e'
  surface-container-low: '#1c1b1b'
  surface-container: '#201f1f'
  surface-container-high: '#2a2a2a'
  surface-container-highest: '#353534'
  on-surface: '#e5e2e1'
  on-surface-variant: '#b9cacb'
  inverse-surface: '#e5e2e1'
  inverse-on-surface: '#313030'
  outline: '#849495'
  outline-variant: '#3b494b'
  surface-tint: '#00dbe9'
  primary: '#dbfcff'
  on-primary: '#00363a'
  primary-container: '#00f0ff'
  on-primary-container: '#006970'
  inverse-primary: '#006970'
  secondary: '#ebb2ff'
  on-secondary: '#520072'
  secondary-container: '#b600f8'
  on-secondary-container: '#fff6fc'
  tertiary: '#f8f5f5'
  on-tertiary: '#313030'
  tertiary-container: '#dcd9d8'
  on-tertiary-container: '#5f5e5e'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#7df4ff'
  primary-fixed-dim: '#00dbe9'
  on-primary-fixed: '#002022'
  on-primary-fixed-variant: '#004f54'
  secondary-fixed: '#f8d8ff'
  secondary-fixed-dim: '#ebb2ff'
  on-secondary-fixed: '#320047'
  on-secondary-fixed-variant: '#74009f'
  tertiary-fixed: '#e5e2e1'
  tertiary-fixed-dim: '#c8c6c5'
  on-tertiary-fixed: '#1c1b1b'
  on-tertiary-fixed-variant: '#474746'
  background: '#131313'
  on-background: '#e5e2e1'
  surface-variant: '#353534'
typography:
  display-lg:
    fontFamily: Space Grotesk
    fontSize: 48px
    fontWeight: '700'
    lineHeight: '1.1'
    letterSpacing: -0.02em
  headline-md:
    fontFamily: Space Grotesk
    fontSize: 24px
    fontWeight: '500'
    lineHeight: '1.2'
  body-base:
    fontFamily: Manrope
    fontSize: 16px
    fontWeight: '400'
    lineHeight: '1.6'
  label-sm:
    fontFamily: Inter
    fontSize: 12px
    fontWeight: '600'
    lineHeight: '1'
    letterSpacing: 0.05em
  code-md:
    fontFamily: JetBrains Mono
    fontSize: 14px
    fontWeight: '400'
    lineHeight: '1.5'
rounded:
  sm: 0.125rem
  DEFAULT: 0.25rem
  md: 0.375rem
  lg: 0.5rem
  xl: 0.75rem
  full: 9999px
spacing:
  base: 8px
  xs: 4px
  sm: 12px
  md: 24px
  lg: 48px
  xl: 80px
  gutter: 16px
  margin: 24px
---

## Brand & Style

This design system targets developers and high-end power users who demand a workspace that feels like a sophisticated cockpit. The personality is a fusion of **Glassmorphism** and **Technical Minimalism**, evoking the precision of a high-end IDE mixed with the cinematic elegance of futuristic interfaces.

The emotional response should be one of "effortless power"—a calm, focused environment where the interface recedes to let the code and AI interactions take center stage. By utilizing deep translucency and razor-thin lines, the UI maintains a lightweight footprint despite its feature density.

## Colors

The palette is anchored in **Premium Dark Mode**. The background isn't a solid black, but a deep, translucent gray that allows subtle hints of the user's desktop to bleed through via heavy Gaussian blur.

- **Primary (Electric Cyan):** Used for terminal cursors, active execution states, and successful deployments.
- **Secondary (Neon Violet):** Used for AI-driven insights, agent suggestions, and deep-link navigation.
- **Neutral/Background:** Layered grays that differentiate the sidebar from the main terminal canvas.
- **Status Colors:** Use the primary cyan for "Go" and the secondary violet for "Processing." Errors should use a muted, desaturated red (#FF4B4B) to avoid breaking the sci-fi aesthetic.

## Typography

The typography strategy employs a strict separation between **UI Narrative** and **Data Input**.

1.  **UI Elements:** Use **Space Grotesk** for headings to provide a futuristic, geometric flair. **Manrope** is used for body copy to ensure legibility during long sessions of reading documentation or logs.
2.  **The Terminal:** All code snippets, CLI inputs, and AI agent reasoning logs must use a modern monospace font (e.g., JetBrains Mono). 
3.  **Labels:** Smaller metadata (like file paths or git branches) use **Inter** for maximum clarity at small scales.

## Layout & Spacing

This design system utilizes a **Fluid Grid** within a multi-pane desktop architecture. The layout is inspired by IDE tiling systems where panes can be collapsed or resized.

- **Rhythm:** A strict 8px baseline grid governs all spacing. 
- **Margins:** Desktop application edges maintain a 24px safe area.
- **Panes:** Sidebars are fixed-width (typically 240px–300px), while the central execution area expands fluidly. 
- **Gaps:** Use 1px gaps for internal pane dividers (using the glass border color) and 16px gutters for floating card arrangements.

## Elevation & Depth

Depth is communicated through **Background Blur (Backdrop-filter)** and **Inner Glows** rather than heavy shadows.

- **Level 0 (Background):** Deepest gray, no blur.
- **Level 1 (Main Canvas):** Translucent (0.6 opacity) with 20px blur. 
- **Level 2 (Modals/Popovers):** Higher opacity (0.8), 30px blur, and a 1px solid border at 0.15 opacity to catch the light.
- **Floating States:** Use a soft, ultra-diffused shadow (`0 20px 40px rgba(0,0,0,0.4)`) to separate active AI dialogs from the terminal background.

## Shapes

The shape language is **Soft (0.25rem)**. This provides a professional, precise feel that avoids the "toy-like" appearance of fully rounded corners while remaining more inviting than sharp 90-degree angles.

- **Containers:** 4px (0.25rem) corner radius.
- **Buttons/Inputs:** 4px corner radius for consistency.
- **Status Pips:** Full circles (pill-shaped) for binary indicators (on/off).

## Components

### Buttons
- **Primary:** Background-less with a 1px primary cyan border and a subtle cyan outer glow on hover. Text is uppercase Space Grotesk.
- **Ghost:** White at 0.4 opacity, shifting to 1.0 on hover. No background.

### Input Fields (Command Line)
- No background or border on three sides; only a 1px solid bottom border that glows cyan when focused.
- Prefix icons (like `>`) should be fixed primary color.

### AI Agent Cards
- Glassmorphic backgrounds with a subtle violet "aurora" gradient in the corner to denote AI-generated content.
- 1px semi-transparent borders are mandatory to maintain structure against the dark background.

### Status Indicators (Chips)
- Minimalist line art icons (1.5px stroke weight).
- Active states use a "breathing" animation (opacity oscillation) on a small 8px circle.

### Scrollbars
- Hidden by default, appearing as thin (2px) desaturated violet lines only upon interaction.