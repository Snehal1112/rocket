// Compile-time switches for features that are hidden until they are fixed.
export const FEATURE_FLAGS = {
  // The "Move to..." request menu does not work yet. Set to true after the fix.
  moveRequestToCollection: false,
} as const;
