export default defineNuxtConfig({
  ssr: false,
  compatibilityDate: '2026-07-24',
  devtools: {
    enabled: false,
  },
  css: ['~/assets/css/main.css'],
  app: {
    head: {
      title: 'Adscope Center',
      meta: [
        {
          name: 'viewport',
          content: 'width=device-width, initial-scale=1',
        },
      ],
    },
  },
  nitro: {
    preset: 'static',
  },
})
