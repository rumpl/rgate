# Comments and frames

[Component index](README.md) · [Interface](../guides/interface.md)

These components have **no electrical pins and no simulation behavior**. They
help explain and organize a circuit.

## Comment

![Comment as rendered in RGate](../images/components/comment.png)

Place Comment using Make, the palette, or T. Properties has **Text**; the
current single-line field represents newlines as `\n`.

Text may contain safe TkGate-style HTML markup:

```html
<h2>Datapath</h2>
<b>Accumulator</b><br />
<font color="red" size="4">Watch this bus</font>
<code>R0</code>
<a href="https://example.com">Documentation</a>
<a href="module:RegisterFile">Open register file</a>
<img src="diagram.png" width="70" height="60" alt="Diagram" />
```

Supported display runs include headings, bold/strong, italic/emphasis, code/pre,
font color/size, line breaks, paragraphs, lists, simple table-cell spacing,
links, and image alt-text placeholders. Common named colors and hex colors work.
Markup source is preserved in the document; it isn't executed as a program.

**Cmd/Ctrl-click or double-click** a link:

- HTTP(S) opens the URL through the platform.
- `module:Name` navigates to a module definition (or a uniquely resolvable live
  instance).
- A `.v`/`.rgate` relative link resolves beside a desktop-opened document. Save
  current edits first; unsaved changes block file-link replacement.
- Browser file links cannot access your filesystem; use File → Open/upload.

Image tags display **alt text placeholders**; image files are not loaded from
the network or disk. The obsolete bundled TkGate image catalog has been removed.
Scripts and style blocks are ignored; this is not a general HTML/CSS browser or
a TkGate tutorial script runner. Colors/sizes/layout are the supported subset,
not full web semantics.

## Frame

![Frame as rendered in RGate](../images/components/frame.png)

Place Frame to draw a background rectangle behind circuit content. Set title in
Properties, or leave the text empty to display its instance name. Set frame
width/height, or drag its **bottom-right corner** in Select mode. Dimensions are
constrained to 20–10000 world-coordinate units.

Frames don't block clicks in their interior. Select/move them via their border;
their resize gesture is undoable. They aren't a grouping container: moving a
frame does not automatically move enclosed gates, and contained gates still
belong to their module.

Frames are ignored by simulation and executable Verilog behavior. Their visual
geometry survives native saves/layout metadata. They do not provide memory
protection, hierarchy, or electrical isolation.
