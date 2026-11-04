import { anyone, screen } from 'forjr/ui'
import './home.css'

export const homeScreen = screen('homeScreen', {
  path: '/',
  title: 'Lights Out',
  access: anyone('the page of the product is public'),
  view: () => (
    <main className="page">
      <div className="box">
        <h1 className="title">LIGHTS OUT</h1>
        <p className="lead">Your computer switches off. Only your AI agents' terminal keeps running, for hours longer on battery.</p>
        <div className="label">Windows (PowerShell)</div>
        <pre className="cmd">irm https://lightsout.sh | iex</pre>
        <div className="label">Linux and Mac</div>
        <pre className="cmd">curl -fsSL https://lightsout.sh | sh</pre>
        <p>
          <a className="link" href="https://github.com/ArynBuilds/lightsout">github.com/ArynBuilds/lightsout</a>
        </p>
      </div>
    </main>
  ),
})
