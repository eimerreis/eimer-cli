Write and publish a product update post for the changes below.

## Changes in this post

{{changes}}

Platforms: {{platforms}}

## Files

- Draft (already created): `{{draft}}`. Its front matter lists the selected changes; keep `changes` and `releases` as they are.
- Other languages to create: {{translations}}. Same front matter, translated title and text.
- Media folder: `{{media_dir}}`, served at `{{media_url}}`.

## 1. Understand the change

Read the commits behind each change (`git show <id>`) and the code they touch, so the post describes what users can now do, not how it was built. Read the website's design and product docs (`DESIGN.md`, `PRODUCT.md` or similar) for voice and claims.

## 2. Write the post

- Title: the feature in a few words, as users would name it. If it differs from the draft's title, rename the post files to the new slug (keep the date prefix) and use that slug for the media folder too.
- Body: 1–3 short paragraphs. What changed, why it helps, where to find it. Direct and encouraging; no internal names, ticket ids, framework names or claims beyond the changes.
- Smaller items go under a final `## Also in this release` list.
- Languages: {{languages}}. Write each one natively, not as a literal translation.

## 3. Media (requested: {{media}})

- `screenshot`, or `auto` for most changes: capture real screens of the sample/demo data, never real user data. Find a UI test that covers the feature and run it; if none exists, add a focused UI test that reaches the screen. Capture light and dark appearance.
- `demo`, or `auto` when the feature is a flow where motion is the point: build an animated demo with the Stageshow skill and reference the component.
- `none`: no media.

Capture commands (`{test}` is the UI test to run):
{{capture}}

Save files to `{{media_dir}}/` with descriptive names (`planner-light.png`, `planner-dark.png`). Crop to the relevant area; keep PNGs under 1 MB.

Refresh App Store / Mac App Store screenshots too: {{store}}. If yes, use the app-store-screenshots skill with the same captures, in its own project folder, separate from this post.

## 4. Front matter when done

```yaml
media:
  kind: image            # image | video | demo | none
  src: {{media_url}}/<file>-light.png
  dark: {{media_url}}/<file>-dark.png
  alt: <what the screenshot shows>
status: published
```

For `demo`, set `kind: demo` and `component: <path to the component>` instead of `src`/`dark`.

## 5. Verify and ship

1. `eimer product check` passes.
2. The website builds and the post renders in light and dark mode, on desktop and phone width.
3. Add a changeset for the website release (patch).
4. Open a pull request titled `docs(updates): {{title}}` with the rendered post's screenshots in the description.
