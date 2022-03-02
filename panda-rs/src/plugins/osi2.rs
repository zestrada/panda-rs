//! Bindings and helpers for working with the OSI2 plugin, allowing kernel
//! introspection via Volatility 3 Profiles.
//!
//! See [`OsiType`] and [`osi_static`] for high-level usage.
//!
//! [`OsiType`]: macro@panda::plugins::osi2::OsiType
//! [`osi_static`]: panda::plugins::osi2::osi_static
use crate::mem::read_guest_type;
use crate::plugin_import;
use crate::prelude::*;
use crate::GuestReadFail;

use std::ffi::CString;
use std::os::raw::c_char;

mod osi_statics;
pub use osi_statics::*;

#[doc(inline)]
/// A macro for declaring global kernel data structures accessible via OSI2. The
/// type of which must implement/derive [`OsiType`], which is pulled from the currently
/// loaded Volatility Profile.
///
/// The static provides one main method: `read`, which takes an [`&mut CPUState`](CPUState)
/// and returns a `Result<T, GuestReadFail>`, where `T` is the type of the static.
///
/// Also provided for structs which derive [`OsiType`] is an accessor method for each
/// field.
///
/// For more information, see the [`OsiType`] derive macro.
///
/// ## Attributes
///
/// * `symbol` (required) - specify the symbol within the volatility profile that describes
/// the storage location of the given type. Takes the form of `#[symbol = "..."]`.
/// * `per_cpu` (optional) - specify that the given symbol is a CPU-local kernel structure and
/// should be handled accordingly
///
/// ## Example
///
/// ```
/// use panda::plugins::osi2::{OsiType, osi_static};
///
/// #[derive(OsiType, Debug)]
/// #[osi(type_name = "task_struct")]
/// struct TaskStruct {
///     comm: [u8; 0x10],
/// }
///
/// osi_static! {
///     #[per_cpu]
///     #[symbol = "current_task"]
///     static CURRENT_TASK: TaskStruct;
/// }
///
/// # let cpu = unsafe { &mut *panda::sys::get_cpu() };
/// // Read the entire structure
/// let current_task = CURRENT_TASK.read(cpu).unwrap();
///
/// // Read a single field `comm`
/// let process_name = CURRENT_TASK.comm(cpu).unwrap();
/// ```
///
/// [`OsiType`]: macro@OsiType
pub use panda_macros::osi_static;

/// A derive macro for allowing a given structure to be used as a type for OS introspection.
///
/// The recommended usage is to declare instances of these types using the [`osi_static`]
/// macro, however [`OsiType::osi_read`] is also available for when an OS data structure
/// is not global.
///
/// ## Attributes
///
/// |     Name    | Field/Struct Level | Required | Description |
/// |:-----------:|:------------------:|:--------:|:------------|
/// | `type_name` |    Struct-Level    |    ✔️     | Sets the name of the type to pull info from within the volatility profile |
/// |   `rename`  |    Field-Level     |          | By default the name of the field within the volatility profile will be assumed to be identical to the field within the Rust type, the `rename` attribute allows overriding this to have the volatility name and Rust field name be separate.
///
/// ## Example
///
/// ```
/// #[derive(OsiType, Debug)]
/// #[osi(type_name = "task_struct")]
/// struct TaskStruct {
///     #[osi(rename = "comm")]
///     process_name: [u8; 0x10],
/// }
/// ```
///
/// ## How it works
///
/// OSI 2 is based around a system of using volatility 3 profiles (also known as "Symbol Tables")
/// in order to have a semantic understanding of operating system types, in order to leverage
/// the infrastructure of memory forensics to enable high-quality runtime analysis.
///
/// To work with these profiles directly would require parsing them, extracting the
/// offsets/sizes/etc of the data types of interest to the user, and then manually
/// performing address/offset calculations before reading kernel memory and then parsing
/// the resulting bytes. This results in a lot of boilerplate, poor ergonomics, and hard
/// to read and maintain code.
///
/// The goal of this derive macro is to handle address calculation of both global and
/// per-CPU symbols as well as handle pulling symbols from the Volatility Profile and
/// even handling the parsing of bytes from memory.
///
/// The `OsiType` derive macro generates two things:
///
/// 1. It generates an implementation of the [`OsiType`](trait@OsiType) trait for your
/// given type. This specifies how to read the entirety of the type from memory.
///
/// 2. It generates a "method delegator" type. This type has one function: hold onto
/// the symbol of an instance of the structure as well as whether or not the given
/// symbol is per-CPU (such as the current process) or OS-global (such as the syscall
/// table). It then provides a set of methods, one for each field of the type `OsiType` is
/// being derived for. This allows for reading individual fields of a structure without
/// parsing the entire type out of memory.
///
/// To create an instance of the method delegator type, the following can be done:
///
/// ```
/// let symbol = "current_task";
/// let is_per_cpu = true;
///
/// let delegate = <T as OsiType>::MethodDelegator::new(symbol, is_per_cpu);
/// ```
///
/// This is what allows for the [`osi_static`] macro to be used in order to read individual
/// fields of a given type.
pub use panda_macros::OsiType;

plugin_import! {
    /// Raw bindings to the osi2 plugin. It is not recommended to use these directly
    static OSI2: Osi2 = extern "osi2" {
        fn kaslr_offset(cpu: &mut CPUState) -> target_ptr_t;
        fn current_cpu_offset(cpu: &mut CPUState) -> target_ulong;

        fn enum_from_name(name: *const c_char) -> Option<&'static VolatilityEnum>;
        fn base_type_from_name(name: *const c_char) -> Option<&'static VolatilityBaseType>;
        fn symbol_from_name(name: *const c_char) -> Option<&'static VolatilitySymbol>;
        fn type_from_name(name: *const c_char) -> Option<&'static VolatilityStruct>;
        fn symbol_addr_from_name(name: *const c_char) -> target_ptr_t;
        fn addr_of_symbol(symbol: &VolatilitySymbol) -> target_ptr_t;
        fn offset_of_field(
            vol_struct: &VolatilityStruct,
            name: *const c_char
        ) -> target_long;
        fn size_of_struct(vol_struct: &VolatilityStruct) -> target_ulong;
    };
}

// See https://doc.rust-lang.org/nomicon/ffi.html#representing-opaque-structs for
// more info on why this is the way it is.
macro_rules! opaque_types {
    ($($(#[$meta:meta])* $name:ident),*) => {
        $(
            $(#[$meta])*
            ///
            /// **Note:** This type is opaque due to having an undefined layout and thus
            /// may only be accessed behind a reference.
            pub struct $name {
                _data: [u8; 0],
                _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
            }
        )*
    };
}

opaque_types! {
    /// An enum within a volatility profile
    ///
    /// Can be obtained via the [`enum_from_name`] function.
    VolatilityEnum,

    /// A base/primitive type within a volatility profile
    ///
    /// Can be obtained via the [`base_type_from_name`] function.
    VolatilityBaseType,

    /// A global symbol declared within the loaded volatility profile
    ///
    /// Can be obtained via the [`symbol_from_name`] function.
    VolatilitySymbol,

    /// An opaque type representing the layout of a given type within the guest OS
    ///
    /// Can be obtained via the [`type_from_name`] function.
    VolatilityStruct
}

impl VolatilitySymbol {
    /// Get the address of the given symbol relative to the KASLR offset. Note that
    /// additional calculations may be required afterwards to handle per-CPU structs.
    pub fn addr(&self) -> target_ptr_t {
        OSI2.addr_of_symbol(self)
    }
}

impl VolatilityStruct {
    /// Get the size of the given type in bytes
    pub fn size(&self) -> target_ulong {
        OSI2.size_of_struct(self)
    }

    /// Get the offset of a given field within the structure given the name of the field
    pub fn offset_of(&self, field: &str) -> target_long {
        let field_name = CString::new(field).unwrap();

        OSI2.offset_of_field(self, field_name.as_ptr())
    }
}

/// Get a reference to an opaque object for accessing information about a given enum based
/// on the volatility symbols currently loaded by OSI2
pub fn enum_from_name(name: &str) -> Option<&'static VolatilityEnum> {
    let name = CString::new(name).unwrap();

    OSI2.enum_from_name(name.as_ptr())
}

/// Get a reference to an opaque object for accessing information about a given base type
/// from the volatility symbols currently loaded by OSI2
pub fn base_type_from_name(name: &str) -> Option<&'static VolatilityBaseType> {
    let name = CString::new(name).unwrap();

    OSI2.base_type_from_name(name.as_ptr())
}

/// Get a reference to an opaque object for accessing information about a given symbol
/// present in the volatility symbols currently loaded by OSI2
pub fn symbol_from_name(name: &str) -> Option<&'static VolatilitySymbol> {
    let name = CString::new(name).unwrap();

    OSI2.symbol_from_name(name.as_ptr())
}

/// Get a reference to an opaque object for accessing information about a given type
/// present in the volatility symbols currently loaded by OSI2
pub fn type_from_name(name: &str) -> Option<&'static VolatilityStruct> {
    let name = CString::new(name).unwrap();

    OSI2.type_from_name(name.as_ptr())
}

/// Get the symbol of a type relative to the KASLR base offset from the volatility profile
/// currently loaded by OSI2. This offset may need additional modification if it points
/// to a per-CPU structure.
pub fn symbol_addr_from_name(name: &str) -> target_ptr_t {
    let name = CString::new(name).unwrap();

    OSI2.symbol_addr_from_name(name.as_ptr())
}

/// Get the KASLR offset of the system, calculating and caching it if it has not already
/// been found. For systems without KASLR this will be 0.
pub fn kaslr_offset(cpu: &mut CPUState) -> target_ptr_t {
    OSI2.kaslr_offset(cpu)
}

/// Get the current per-CPU offset for kernel data structures such as the current task
/// struct
pub fn current_cpu_offset(cpu: &mut CPUState) -> target_ulong {
    OSI2.current_cpu_offset(cpu)
}

/// Get the address from a given symbol
pub fn addr_of_symbol(symbol: &VolatilitySymbol) -> target_ptr_t {
    OSI2.addr_of_symbol(symbol)
}

/// Get the offset of a field given the structure it is within and the name of the field
pub fn offset_of_field(vol_struct: &VolatilityStruct, name: &str) -> target_long {
    let name = CString::new(name).unwrap();

    OSI2.offset_of_field(vol_struct, name.as_ptr())
}

/// Get the size of a given structure
pub fn size_of_struct(vol_struct: &VolatilityStruct) -> target_ulong {
    OSI2.size_of_struct(vol_struct)
}

/// Get the per-cpu address for a given symbol where the underlying type is stored
pub fn find_per_cpu_address(
    cpu: &mut CPUState,
    symbol: &str,
) -> Result<target_ptr_t, GuestReadFail> {
    let symbol_offset = symbol_addr_from_name(symbol);
    let ptr_to_ptr = kaslr_offset(cpu) + current_cpu_offset(cpu) + symbol_offset;

    read_guest_type(cpu, ptr_to_ptr)
}