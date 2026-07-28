// Example web application entry point
const express = require('express');
const app = express();

app.get('/', (req, res) => {
  res.send('Hello from the Lattice web example!');
});

app.listen(3000, () => {
  console.log('Web app running on http://localhost:3000');
});
