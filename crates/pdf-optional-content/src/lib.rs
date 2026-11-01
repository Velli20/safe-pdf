//! Optional content groups: the catalog's `/OCProperties`, the `/OCG` and `/OCMD`
//! references that make page content and annotations conditional, and the runtime
//! visibility state that decides whether they are shown.

mod decode;
/// Errors decoding optional content declarations.
pub mod error;
/// Optional content group state operations of `SetOCGState` actions.
pub mod ocg_state;
/// Optional content group visibility applied to page content and annotations.
pub mod optional_content;
/// `/VE` visibility expressions of optional content membership dictionaries.
pub mod visibility_expression;
mod wire;

pub use error::OptionalContentError;
pub use ocg_state::{OcgOperation, OcgState, OcgStateChange, OcgTarget};
pub use optional_content::{
    OptionalContentBaseState, OptionalContentConfiguration, OptionalContentGroup,
    OptionalContentGroupId, OptionalContentMembership, OptionalContentPolicy,
    OptionalContentProperties, OptionalContentState,
};
pub use visibility_expression::VisibilityExpression;
