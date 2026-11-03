import 'dotenv/config';
import express from 'express';
import path from 'node:path';
import fs from 'node:fs';
import { carregarConfig } from './fiscal/config.js';
import { lerCertificado } from './fiscal/certificado.js';
import { rotas } from './routes.js';

const config = carregarConfig(); // falha se não for homologação
const certificado = lerCertificado(config.certPath, config.certSenha);
const porta = Number(process.env.PORT || 3335);

const app = express();
app.use(express.json({ limit: '2mb' }));
app.use('/api', (req, res, next) => {
  res.set('Cache-Control', 'no-store');
  next();
});
app.use('/api', rotas({ config, certificado }));

const dist = path.resolve('dist');
if (fs.existsSync(dist)) {
  app.use(express.static(dist));
  app.get('*', (req, res) => res.sendFile(path.join(dist, 'index.html')));
}

// Só localhost: esta API assina e envia notas com o certificado carregado.
app.listen(porta, '127.0.0.1', () => {
  console.log(`Emissor NF-e (HOMOLOGAÇÃO, ${config.uf}) em http://localhost:${porta}`);
  console.log(`Certificado: ${certificado.info.titular} (${certificado.info.tipo})`);
});
