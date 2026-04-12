import tailwindcss from "@tailwindcss/vite";
const config = {
  server: {
    host: "0.0.0.0",
    port: 3e3,
    origin: "https://docs.dinoco.io"
  },
  vite: {
    plugins: [tailwindcss()],
    assetsInclude: ["**/*.md"]
  }
};
export {
  config as default
};
