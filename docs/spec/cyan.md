# `cyan` File Format

This document specifies `cyan` format version `0` (pre-alpha). A `cyan` file is a single SQLite 3 database with no additional header or outer container.

Third-party plugins may add tables. This specification defines only the standard built-in tables.

UUID values are stored in the canonical 16-octet binary representation defined by RFC 9562, with each field in network byte order.

## `metadata`

```sql
CREATE TABLE metadata (
    version INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX metadata_singleton ON metadata ((1));
```

`metadata` contains exactly one row.

| Column    | Rust type | Meaning                        |
| --------- | --------- | ------------------------------ |
| `version` | `u32`     | Schema version of this archive |

## `image`

```sql
CREATE TABLE image (
    width         INTEGER NOT NULL,
    height        INTEGER NOT NULL,
    tile_size     INTEGER NOT NULL,
    color_profile BLOB NOT NULL,
    root_layer    BLOB NOT NULL CHECK (length(root_layer) = 16),
    texel_type    INTEGER NOT NULL
);

CREATE UNIQUE INDEX image_singleton ON image ((1));
```

`image` contains exactly one row.

| Column          | Rust type | Meaning                                                  |
| --------------- | --------- | -------------------------------------------------------- |
| `width`         | `u32`     | Image width in pixels                                    |
| `height`        | `u32`     | Image height in pixels                                   |
| `tile_size`     | `u32`     | Side length of a square tile in pixels                   |
| `color_profile` | `Vec<u8>` | Embedded ICC profile bytes                               |
| `root_layer`    | `Uuid`    | Root layer ID bytes                                      |
| `texel_type`    | `u8`      | Encoded `TexelType`. The texel type of the entire image. |

## `layer_tree`

```sql
CREATE TABLE layer_tree (
    id         BLOB PRIMARY KEY NOT NULL CHECK (length(id) = 16),
    parent_id  BLOB CHECK (parent_id IS NULL OR length(parent_id) = 16),
    sort_order INTEGER,
    layer_type INTEGER NOT NULL,
    properties BLOB NOT NULL,
    CHECK (
        (parent_id IS NULL AND sort_order IS NULL)
        OR
        (parent_id IS NOT NULL AND sort_order IS NOT NULL)
    )
) WITHOUT ROWID;

CREATE INDEX layer_tree_parent ON layer_tree (parent_id, sort_order);
```

| Column       | Rust type      | Meaning                                                                  |
| ------------ | -------------- | ------------------------------------------------------------------------ |
| `id`         | `Uuid`         | Layer ID bytes                                                           |
| `parent_id`  | `Option<Uuid>` | Parent layer ID; `NULL` for the root                                     |
| `sort_order` | `Option<u32>`  | Zero-based position within the parent; `NULL` for the root               |
| `layer_type` | `u32`          | Global unique layer type identifier                                      |
| `properties` | `Vec<u8>`      | MessagePack map from property identifier strings to encoded byte arrays |

- `parent_id` and `sort_order` must either both be `NULL` or both be non-`NULL`.
- There is one and only one root layer, whose `parent_id` and `sort_order` are both `NULL`.
- `image.root_layer` must reference the root layer.
- Every non-root layer must reference an existing parent and be reachable from the root.
- The `sort_order` values under each parent must be unique and form a contiguous sequence starting at `0`.
- Every property listed for a built-in layer type is required. Each byte array in the `properties` map contains one independently MessagePack-encoded property value. Map entry order has no meaning.

## `tile_data`

```sql
CREATE TABLE tile_data (
    layer_id BLOB NOT NULL CHECK (length(layer_id) = 16),
    tile_x   INTEGER NOT NULL,
    tile_y   INTEGER NOT NULL,
    data     BLOB NOT NULL,
    PRIMARY KEY (layer_id, tile_x, tile_y)
) WITHOUT ROWID;
```

| Column     | Rust type | Meaning                                          |
| ---------- | --------- | ------------------------------------------------ |
| `layer_id` | `Uuid`    | Owning layer ID bytes                            |
| `tile_x`   | `i32`     | Signed horizontal tile index                     |
| `tile_y`   | `i32`     | Signed vertical tile index                       |
| `data`     | `Vec<u8>` | Raw DEFLATE-compressed pixel data for one tile   |

- Only layers that contain pixels have entries in `tile_data`, and every `layer_id` must reference an existing layer.
- `data` is an RFC 1951 DEFLATE stream without a zlib or gzip wrapper.
- Decompressed pixels are tightly packed in row-major order without row padding. The x coordinate increases within each row, followed by the y coordinate.
- Alpha/8-bit tiles store one alpha byte per pixel. RGBA/8-bit tiles store four bytes per pixel in R, G, B, A order.
- Every entry stores a complete `tile_size` by `tile_size` tile, including tiles at image edges.

## Built-in layer types

| `layer_type` | Layer Type Name | Layer Properties                                                                                           |
| ------------ | --------------- | ---------------------------------------------------------------------------------------------------------- |
| `0`          | Pixel layer     | `name`, `visible`, `blend_function`, `opacity`, `locked`, `locked_channels`, `disabled_channels`, `texel_type` |
| `1`          | Group layer     | `name`, `visible`, `blend_function`, `opacity`, `locked`, `disabled_channels`                                 |

## Built-in layer properties

| Property            | Type                | Description                                                                                                                                                                           |
| ------------------- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `visible`           | `bool`              | Whether the layer is visible.                                                                                                                                                         |
| `opacity`           | `f32`               | Opacity of the layer (0.0 to 1.0).                                                                                                                                                    |
| `blend_function`    | `string`            | Id of the blend function to use.                                                                                                                                                      |
| `name`              | `string`            | Name of the layer.                                                                                                                                                                    |
| `locked`            | `bool`              | Whether the layer is locked. Tools must not modify locked layers.                                                                                                                      |
| `disabled_channels` | `u32`               | Bitmask of channels excluded from blending. For example, disabling alpha implements alpha inheritance.                                                                                |
| `locked_channels`   | `u32`               | Encoded bitmask of locked channels. Pixels inside the locked channels cannot be modified.                                                                                             |
| `texel_type`        | `TexelType`         | Texel type of the layer.                                                                                                                                                              |

For `disabled_channels` and `locked_channels`, RGBA uses bits 0, 1, 2, and 3 for R, G, B, and A respectively. Alpha uses bit 0. All other bits are reserved.

Layer properties are plain data. They do not directly constrain the behavior of other application components.

## Texel types

`TexelType` is an unsigned 8-bit integer with the following layout:

| Bits | Meaning |
| ---- | ------- |
| 7..4 | Format  |
| 3..0 | Depth   |

| Format  | Value |
| ------- | ----- |
| `Alpha` | `0`   |
| `RGBA`  | `1`   |

| Depth   | Value |
| ------- | ----- |
| `8-bit` | `0`   |

`0x00` is Alpha/8-bit. `0x10` is RGBA/8-bit.

All values not listed above are invalid. `TexelType` is exhaustive and cannot be extended externally.
