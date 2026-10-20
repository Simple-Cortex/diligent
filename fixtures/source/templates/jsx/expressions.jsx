import { Icon } from './icon';

export function Card({ user, ok, save, label }) {
  return (
    <>
      <h1>One</h1>
      <img src={user.photo} alt={user.name} />
      <img src="/missing.png" />
      <img {...user.imgProps} />
      <Icon name="star" />
      <a href="/profile"><Icon name="star" /></a>
      <button onClick={save}>{label}</button>
      {ok && <h3>Shown only sometimes</h3>}
      {ok ? <p id="note">First</p> : <p id="note">Second</p>}
    </>
  );
}
