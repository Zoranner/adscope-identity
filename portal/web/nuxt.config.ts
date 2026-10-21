import tailwindcss from '@tailwindcss/vite'

export default defineNuxtConfig({
  ssr: false,
  compatibilityDate: '2026-07-24',
  modules: ['@nuxt/icon', '@stellar/ui/nuxt'],
  icon: {
    provider: 'none',
    componentName: 'NuxtIcon',
    clientBundle: {
      scan: {
        globInclude: ['**/*.{vue,jsx,tsx,ts,md,mdc,mdx,yml,yaml}'],
      },
    },
  },
  devtools: {
    enabled: false,
  },
  css: ['~/assets/css/main.css'],
  vite: {
    plugins: [tailwindcss()],
  },
  app: {
    head: {
      title: 'Adscope Portal',
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
