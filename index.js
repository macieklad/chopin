// Require the framework and instantiate it
const fastify = require('fastify')({ logger: true })
const path = require('path')
const fs = require('fs')
const { exec } = require('child_process');

fastify.register(require('fastify-static'), {
  root: path.join(__dirname, 'public'),
  prefix: '/public/',
})

fastify.get('/', async (request, reply) => {
  return reply.sendFile('index.html');
})

fastify.post('/interpret', async (request, reply) => {
  return new Promise(resolve => {
    fs.writeFileSync('code.chp', request.body);
    exec('cargo run code.chp > result', () => {
      const result = fs.readFileSync('result', 'utf-8')
      resolve(result)
    })
  })
})

// Run the server!
const start = async () => {
  try {
    await fastify.listen(3000)
  } catch (err) {
    fastify.log.error(err)
    process.exit(1)
  }
}
start()