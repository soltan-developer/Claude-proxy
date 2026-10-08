declare module "*.png" {
  const source: string;
  export default source;
}

declare module "*.txt?raw" {
  const content: string;
  export default content;
}
