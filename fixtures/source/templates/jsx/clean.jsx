import React from 'react';

export default function Page() {
  const count = 3;
  return (
    <main>
      <h1>Annual report</h1>
      <img src="/chart.png" alt="Bar chart of 2025 totals" />
      <label htmlFor="email">Email address</label>
      <input id="email" type="email" />
      <a href="/privacy">Privacy policy</a>
    </main>
  );
}
