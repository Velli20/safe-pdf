/** Readable dispatch names, statically checked against the generated Rust contract. */
/** @satisfies {Record<string, import('./annotation_contract').AnnotationKind['kind']>} */
export const AnnotationKind = Object.freeze({ Highlight: 'highlight', Note: 'text_note', Ink: 'ink', Widget: 'widget', FreeText: 'free_text', Link: 'link', Line: 'line', Square: 'square', Circle: 'circle', Polygon: 'polygon', PolyLine: 'poly_line', Underline: 'underline', Squiggly: 'squiggly', StrikeOut: 'strike_out', Stamp: 'stamp', Caret: 'caret', Popup: 'popup', Unknown: 'unknown' });
/** @satisfies {Record<string, import('./annotation_contract').ControlKind>} */
export const ControlKind = Object.freeze({ Text: 'text', Checkbox: 'checkbox', CheckboxGroup: 'checkbox_group', Radio: 'radio', ListBox: 'list_box', ComboBox: 'combo_box', Button: 'button' });
