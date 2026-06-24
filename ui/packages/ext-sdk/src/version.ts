/** The SDK contract version the host advertises (design §4.3 version
 *  negotiation). A remote declares the range it is built against in its
 *  manifest `frontend.sdk_range`; the host refuses to mount a remote whose
 *  range does not satisfy this version. Bump the **major** on a breaking SDK
 *  surface change, the **minor** on additive changes. */
export const SDK_VERSION = "0.1.0";
