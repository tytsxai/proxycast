export default {
  extends: ["docus"],
  app: {
    baseURL: "/proxycast/",
  },
  image: {
    provider: "none",
  },
  robots: {
    robotsTxt: false,
  },
  llms: {
    domain: "https://proxycast.local",
  },
};
