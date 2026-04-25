/// <reference types="vite/client" />

// Raw text imports — used for the parrot.live ASCII frames so the source-of-
// truth lives in side-by-side .txt files instead of escaped string literals.
declare module "*.txt?raw" {
  const content: string;
  export default content;
}
