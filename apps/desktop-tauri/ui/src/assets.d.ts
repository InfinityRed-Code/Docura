// Vite bundles image imports as URLs (tsconfig has `types: []`, so the
// vite/client declarations aren't available).
declare module "*.png" {
  const url: string;
  export default url;
}

declare module "*.svg" {
  const url: string;
  export default url;
}
