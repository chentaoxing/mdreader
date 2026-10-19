Setext Heading Level 1
======================

This is a paragraph after a Setext heading.

Setext Heading Level 2
----------------------

This is a paragraph after a Setext level 2 heading.

## ATX Heading After Setext

More Setext
===

Even more Setext
---

### Mixed Heading Styles

Another ATX heading
-------------------

Wait, this is actually a paragraph with a trailing `---`, not a Setext heading,
because ATX headings take precedence in CommonMark when the line starts with `#`.

# ATX Level 1

Some text here.

## ATX Level 2

More text.

### ATX Level 3

Even more text.

#### ATX Level 4

And more.

##### ATX Level 5

Almost there.

###### ATX Level 6

Last level.

####### Not a heading (level 7+)

This is just a paragraph because CommonMark only supports up to level 6.

# Heading with trailing hashes ##

The trailing hashes are optional and should be stripped.

# Heading with # inside

This heading contains a hash symbol inside.

#  Empty heading

The heading above has no text content.

Setext Underline Variations
===========================

This underline uses many = characters.

Minimal Setext
=

This uses a single = character for the underline.

Minimal Setext Level 2
-

This uses a single - character.

Setext with spaces in underline
  ====  

The underline can have leading/trailing spaces.

# Heading Level Skip Test

## Jumping to Level 4

#### This skips level 3

###### This skips level 5

# Back to Level 1

### Now Level 3

## Back to Level 2

##### Jump to Level 5

This tests how renderers handle heading level jumps.
