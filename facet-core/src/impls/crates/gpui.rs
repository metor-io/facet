#![cfg(feature = "gpui")]

use gpui::{Entity, Hsla, SharedString};

use crate::{
    Def, Facet, Field, FieldBuilder, OxPtrConst, OxPtrMut, OxPtrUninit, ParseError,
    PointerDef, PointerFlags, PointerVTable, Shape, ShapeBuilder, ShapeFlags, StructKind,
    StructTypeBuilder, Type, TypeNameOpts, TypeOpsIndirect, TypeParam, UserType, VTableIndirect,
    Variance, VarianceDep, VarianceDesc,
};

// --- Hsla ---

// gpui's Hsla is palette's `Alpha<Hsl, f32>`: `#[repr(C)]` all the way down,
// so its four f32s sit at fixed offsets. The hue is stored in degrees.
const HSLA_FIELDS: &[Field] = &[
    FieldBuilder::new(
        "hue",
        crate::shape_of::<f32>,
        core::mem::offset_of!(Hsla, color.hue),
    )
    .build(),
    FieldBuilder::new(
        "saturation",
        crate::shape_of::<f32>,
        core::mem::offset_of!(Hsla, color.saturation),
    )
    .build(),
    FieldBuilder::new(
        "lightness",
        crate::shape_of::<f32>,
        core::mem::offset_of!(Hsla, color.lightness),
    )
    .build(),
    FieldBuilder::new("alpha", crate::shape_of::<f32>, core::mem::offset_of!(Hsla, alpha)).build(),
];

/// The hue as a fraction of the circle, the text form's unit (and gpui's
/// before it took palette's degrees).
fn hue_fraction(c: &Hsla) -> f32 {
    c.color.hue.into_positive_degrees() / 360.0
}

unsafe fn display_hsla(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let c = source.get::<Hsla>();
        Some(write!(
            f,
            "{:.2} {:.2} {:.2} {:.2}",
            hue_fraction(c),
            c.color.saturation,
            c.color.lightness,
            c.alpha
        ))
    }
}

unsafe fn partial_eq_hsla(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<Hsla>();
        let b = b.get::<Hsla>();
        Some(
            a.color.hue == b.color.hue
                && a.color.saturation == b.color.saturation
                && a.color.lightness == b.color.lightness
                && a.alpha == b.alpha,
        )
    }
}

unsafe fn parse_hsla(s: &str, target: OxPtrUninit) -> Option<Result<(), ParseError>> {
    let parts: Vec<f32> = s.split_whitespace().filter_map(|p| p.parse().ok()).collect();
    let alpha = match parts.len() {
        3 => 1.0,
        4 => parts[3],
        _ => {
            return Some(Err(ParseError::from_str(
                "expected 3 or 4 space-separated floats (h s l [a])",
            )));
        }
    };
    unsafe {
        target.put(gpui::hsla(parts[0], parts[1], parts[2], alpha));
    }
    Some(Ok(()))
}

const HSLA_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_hsla),
    partial_eq: Some(partial_eq_hsla),
    parse: Some(parse_hsla),
    ..VTableIndirect::EMPTY
};

// Safety: Hsla is `#[repr(C)]` (palette's Alpha over Hsl over RgbHue) with
// four f32s; the field offsets come from `offset_of!` on Hsla itself. It is
// Copy with no invariants, so POD is correct.
unsafe impl Facet<'_> for Hsla {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Hsla>("Hsla")
            .module_path("gpui")
            .ty(Type::User(UserType::Struct(
                StructTypeBuilder::new(StructKind::Struct, HSLA_FIELDS).build(),
            )))
            .def(Def::Scalar)
            .flags(ShapeFlags::POD)
            .vtable_indirect(&HSLA_VTABLE)
            .build()
    };
}

// --- SharedString ---

unsafe fn display_shared_string(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let s = source.get::<SharedString>();
        Some(write!(f, "{s}"))
    }
}

unsafe fn parse_shared_string(
    s: &str,
    target: OxPtrUninit,
) -> Option<Result<(), ParseError>> {
    unsafe {
        target.put(SharedString::from(s.to_string()));
        Some(Ok(()))
    }
}

unsafe fn partial_eq_shared_string(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<SharedString>();
        let b = b.get::<SharedString>();
        Some(a == b)
    }
}

const SHARED_STRING_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(display_shared_string),
    parse: Some(parse_shared_string),
    partial_eq: Some(partial_eq_shared_string),
    ..VTableIndirect::EMPTY
};

// Safety: SharedString is a single-field wrapper. We expose it as an
// opaque scalar — consumers go through Display/Parse for conversion.
unsafe impl Facet<'_> for SharedString {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<SharedString>("SharedString")
            .module_path("gpui")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&SHARED_STRING_VTABLE)
            .eq()
            .build()
    };
}

// --- Entity<T> ---

fn type_name_entity<'a, T: Facet<'a> + 'static>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Entity")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        T::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<…>")?;
    }
    Ok(())
}

unsafe fn entity_drop<T: 'static>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<Entity<T>>() as *mut Entity<T>);
    }
}

unsafe fn entity_clone<T: 'static>(src: OxPtrConst, dst: OxPtrMut) {
    unsafe {
        let value = src.get::<Entity<T>>().clone();
        (dst.ptr().as_ptr::<Entity<T>>() as *mut Entity<T>).write(value);
    }
}

// Safety: Entity<T> is a ref-counted smart pointer to a gpui-managed
// value of type T. We expose it as an opaque pointer with
// `inner = T::SHAPE` so reflection walkers can see the pointee type.
// No `borrow_fn` is provided because dereferencing requires `&App`
// context — the walker must handle Entity specially via `entity.read(cx)`.
unsafe impl<'a, T: Facet<'a> + 'static> Facet<'a> for Entity<T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Entity<T>>("Entity")
            .module_path("gpui")
            .type_name(type_name_entity::<T>)
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(&const {
                TypeOpsIndirect {
                    drop_in_place: entity_drop::<T>,
                    default_in_place: None,
                    clone_into: Some(entity_clone::<T>),
                    is_truthy: None,
                }
            })
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const { PointerVTable::new() },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::empty(),
                known: None,
            }))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::covariant(T::SHAPE)] },
            })
            .build()
    };
}
