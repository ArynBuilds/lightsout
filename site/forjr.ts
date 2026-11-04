import { asset, project } from 'forjr'
import { installerFor } from './functions/installer-for'

export const lightsout = project('lightsout', {
  title: 'Lights Out',
  icon: asset('icon.svg'),
  // `irm https://lightsout.sh | iex` and `curl -fsSL https://lightsout.sh | sh` get
  // the installer itself; a browser gets the page
  onRequest: async (request, next) => {
    const answer = await installerFor(request)
    return answer ?? next(request)
  },
})
